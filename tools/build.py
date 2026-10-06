import argparse
import os
import shutil
import subprocess
import sys
import zipfile
from abc import ABC, abstractmethod
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import ClassVar, Final, NoReturn, override

ROOT: Final = Path(__file__).resolve().parent.parent
BUILD: Final = ROOT / "build"
DIST: Final = BUILD / "dist"
STAGE: Final = BUILD / "stage"
LIB_SOURCE: Final = ROOT / "lib"
LIB_OUTPUT: Final = BUILD / "lib" / "release"
CARGO_OUTPUT: Final = BUILD / "cargo"
MODULE_SOURCE: Final = ROOT / "modules"

PRESET: Final = "release"
TARGET: Final = "aarch64-linux-android"
LINKER_VARIABLE: Final = "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
API_LEVEL: Final = 29
DAEMON: Final = "cluxd"
MODULE_EXCLUDED: Final = ("update.json",)
EXECUTABLE_NAMES: Final = frozenset({"update-binary", DAEMON})
EXECUTABLE_SUFFIXES: Final = frozenset({".sh"})
ZIP_EPOCH: Final = (1980, 1, 1, 0, 0, 0)
ZIP_LEVEL: Final = 9
MODE_EXECUTABLE: Final = 0o755
MODE_REGULAR: Final = 0o644
MODE_SHIFT: Final = 16

type Environment = Mapping[str, str]


def say(message: str) -> None:
    sys.stdout.write(f"{message}\n")


def fail(message: str) -> NoReturn:
    text = f"error: {message}"
    raise SystemExit(text)


def locate(tool: str) -> str:
    found = shutil.which(tool)
    if found is None:
        fail(f"required tool is not in PATH: {tool}")

    return found


def execute(command: Sequence[str], cwd: Path, env: Environment | None = None) -> None:
    say(f"$ {' '.join(command)}")
    subprocess.run(command, cwd=cwd, env=env, check=True)


def ndk_root() -> Path:
    value = os.environ.get("ANDROID_NDK_HOME")
    if not value:
        fail("ANDROID_NDK_HOME is not set")

    root = Path(value)
    if not root.is_dir():
        fail(f"ANDROID_NDK_HOME does not point to a directory: {root}")

    return root


def ndk_linker(ndk: Path) -> Path:
    prebuilt = ndk / "toolchains" / "llvm" / "prebuilt"

    hosts = sorted(entry for entry in prebuilt.glob("*") if entry.is_dir())
    if not hosts:
        fail(f"no LLVM toolchain found under {prebuilt}")

    executable_name = f"{TARGET}{API_LEVEL}-clang"
    if os.name == "nt":
        executable_name += ".cmd"

    linker = hosts[0] / "bin" / executable_name
    if not linker.is_file():
        fail(f"linker not found: {linker}")

    return linker


def daemon_binary() -> Path:
    return CARGO_OUTPUT / TARGET / "release" / DAEMON


def module_version() -> str:
    properties = (MODULE_SOURCE / "module.prop").read_text(encoding="utf-8")
    for line in properties.splitlines():
        key, separator, value = line.partition("=")
        if separator and key == "version":
            return value.strip()

    return fail("version is missing in modules/module.prop")


def is_executable(path: Path) -> bool:
    return path.name in EXECUTABLE_NAMES or path.suffix in EXECUTABLE_SUFFIXES


def archive(source: Path, target: Path) -> None:
    files = sorted(entry for entry in source.rglob("*") if entry.is_file())

    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=ZIP_LEVEL) as bundle:
        for path in files:
            info = zipfile.ZipInfo(path.relative_to(source).as_posix(), ZIP_EPOCH)
            mode = MODE_EXECUTABLE if is_executable(path) else MODE_REGULAR
            info.external_attr = mode << MODE_SHIFT
            info.compress_type = zipfile.ZIP_DEFLATED
            bundle.writestr(info, path.read_bytes(), compresslevel=ZIP_LEVEL)


class Command(ABC):
    name: ClassVar[str]
    summary: ClassVar[str]

    @abstractmethod
    def run(self) -> None: ...


class Clean(Command):
    name = "clean"
    summary = "remove the build directory"

    @override
    def run(self) -> None:
        shutil.rmtree(BUILD, ignore_errors=True)
        say(f"removed {BUILD}")


class Build(Command):
    name = "build"
    summary = "compile libclux.a and the cluxd daemon"

    @override
    def run(self) -> None:
        ndk = ndk_root()
        cmake = locate("cmake")
        cargo = locate("cargo")
        locate("ninja")

        execute([cmake, "--preset", PRESET], LIB_SOURCE)
        execute([cmake, "--build", "--preset", PRESET], LIB_SOURCE)

        env = {
            **os.environ,
            "CLUX_LIB_DIR": str(LIB_OUTPUT),
            LINKER_VARIABLE: str(ndk_linker(ndk)),
        }
        execute([cargo, "build", "--release", "--locked", "--target", TARGET], ROOT, env)

        DIST.mkdir(parents=True, exist_ok=True)
        shutil.copy2(daemon_binary(), DIST / DAEMON)
        say(f"daemon: {DIST / DAEMON}")


class Mkmod(Command):
    name = "mkmod"
    summary = "build the daemon and package the Magisk module zip"

    @override
    def run(self) -> None:
        Build().run()
        shutil.rmtree(STAGE, ignore_errors=True)
        shutil.copytree(MODULE_SOURCE, STAGE, ignore=shutil.ignore_patterns(*MODULE_EXCLUDED))
        payload = STAGE / "system" / "bin"
        payload.mkdir(parents=True)
        shutil.copy2(daemon_binary(), payload / DAEMON)
        target = DIST / f"cluxd-v{module_version()}.zip"
        target.unlink(missing_ok=True)
        archive(STAGE, target)
        say(f"module: {target}")


COMMANDS: Final[tuple[type[Command], ...]] = (Mkmod, Build, Clean)


def main(argv: Sequence[str] | None = None) -> int:
    registry = {command.name: command for command in COMMANDS}
    listing = "\n".join(f"  {name:<6} {registry[name].summary}" for name in registry)

    parser = argparse.ArgumentParser(
        prog="build.py",
        description="Build tool for clux.",
        epilog=listing,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("command", choices=list(registry), help="action to perform")

    arguments = parser.parse_args(argv)
    try:
        registry[str(arguments.command)]().run()
    except subprocess.CalledProcessError as error:
        return error.returncode

    return 0


if __name__ == "__main__":
    sys.exit(main())
