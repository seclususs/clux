#!/usr/bin/env python3
import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

DEFAULT_ANDROID_API = "33"
DEFAULT_BUILD_TYPE = "Release"
ARCH_ABI = "arm64-v8a"
RUST_TARGET = "aarch64-linux-android"


class Style:
    GREEN = "\033[92m"
    CYAN = "\033[96m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    BOLD = "\033[1m"
    RESET = "\033[0m"


def log_info(msg):
    print(f"{Style.BOLD}[+]{Style.RESET} {msg}")


def log_sub(msg):
    print(f" {Style.CYAN}->{Style.RESET} {msg}")


def log_ok(msg):
    print(f" {Style.GREEN}[OK]{Style.RESET} {msg}")


def log_warn(msg):
    print(f"{Style.YELLOW}[WARN]{Style.RESET} {msg}")


def log_err(msg):
    print(f"\n{Style.RED}[ERROR]{Style.RESET} {msg}")
    sys.exit(1)


def find_ndk():
    ndk_env = os.environ.get("ANDROID_NDK_HOME")

    if ndk_env and os.path.exists(ndk_env):
        return Path(ndk_env)

    home = Path.home()

    if platform.system() == "Windows":
        search_paths = [
            Path(os.environ.get("LOCALAPPDATA", "")) / "Android/Sdk/ndk",
            home / "AppData/Local/Android/Sdk/ndk",
        ]

    elif platform.system() == "Darwin":
        search_paths = [home / "Library/Android/sdk/ndk"]

    else:
        search_paths = [home / "Android/Sdk/ndk"]

    for path in search_paths:
        if path.exists():
            versions = sorted(
                [d for d in path.iterdir() if d.is_dir()],
                reverse=True,
            )

            if versions:
                return versions[0]

    return None


def check_tool(tool_name):
    if not shutil.which(tool_name):
        log_err(f"Missing tool: '{tool_name}'. Please install it.")


def run_cmd(cmd, cwd=None, silent=False):
    try:
        if silent:
            process = subprocess.run(
                cmd,
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
            )

            if process.returncode != 0:
                print(process.stdout)
                raise subprocess.CalledProcessError(process.returncode, cmd)

        else:
            subprocess.run(cmd, check=True, cwd=cwd)

    except subprocess.CalledProcessError as exc:
        log_err(f"Command failed: {' '.join([str(c) for c in cmd])}")
        raise exc


def clean_workspace(root_dir):
    targets = [
        root_dir / "build",
        root_dir / "target",
        root_dir / "scripts/output",
        root_dir / "magisk-module/system/bin/qos_daemon",
    ]

    for path in targets:
        if path.exists():
            try:
                if path.is_dir():
                    shutil.rmtree(path)

                else:
                    path.unlink()

                log_sub(f"Removed: {path.relative_to(root_dir)}")

            except OSError as exc:
                log_warn(f"Failed to clean {path}: {exc}")


def run_quality_checks(ndk_path, api_level, root_dir, do_check, do_lint):
    if not do_check and not do_lint:
        return

    log_info("Running quality checks...")
    rust_path = root_dir / "core"

    if rust_path.exists():
        if do_check:
            run_cmd(
                ["cargo", "check", "--target", RUST_TARGET, "--release"],
                cwd=rust_path,
                silent=True,
            )
            log_ok("Rust syntax")

        if do_lint:
            run_cmd(
                ["cargo", "clippy", "--target", RUST_TARGET, "--release"],
                cwd=rust_path,
                silent=True,
            )
            log_ok("Rust lint")

    build_dir = root_dir / "build" / "Release" / ARCH_ABI
    build_dir.mkdir(parents=True, exist_ok=True)

    toolchain = ndk_path / "build/cmake/android.toolchain.cmake"

    cmake_cmd = [
        "cmake",
        "-Wno-dev",
        f"-DANDROID_ABI={ARCH_ABI}",
        f"-DANDROID_PLATFORM=android-{api_level}",
        f"-DCMAKE_TOOLCHAIN_FILE={toolchain}",
        "-DCMAKE_BUILD_TYPE=Release",
        "-G",
        "Ninja",
        str(root_dir / "native"),
    ]

    try:
        run_cmd(cmake_cmd, cwd=build_dir, silent=True)

    except subprocess.CalledProcessError:
        log_err("Failed to configure CMake for analysis.")

    if do_check:
        run_cmd(["ninja", "syntax"], cwd=build_dir, silent=True)
        log_ok("C++ syntax")

    if do_lint:
        run_cmd(["ninja", "lint"], cwd=build_dir, silent=True)
        log_ok("C++ lint")


def build_daemon(ndk_path, api_level, build_type, root_dir):
    log_info(f"Building Daemon [{build_type}] for {ARCH_ABI} (API {api_level})")

    build_dir = root_dir / "build" / build_type / ARCH_ABI

    if build_dir.exists():
        shutil.rmtree(build_dir)

    build_dir.mkdir(parents=True, exist_ok=True)

    try:
        run_cmd(["rustup", "target", "add", RUST_TARGET], silent=True)

    except subprocess.CalledProcessError:
        log_err(f"Failed to add Rust target: {RUST_TARGET}")

    toolchain = ndk_path / "build/cmake/android.toolchain.cmake"

    cmake_cmd = [
        "cmake",
        "-Wno-dev",
        "-Wno-deprecated",
        f"-DANDROID_ABI={ARCH_ABI}",
        f"-DANDROID_PLATFORM=android-{api_level}",
        f"-DCMAKE_TOOLCHAIN_FILE={toolchain}",
        f"-DCMAKE_BUILD_TYPE={build_type}",
        "-G",
        "Ninja",
        str(root_dir / "native"),
    ]

    log_sub("Configuring CMake...")
    run_cmd(cmake_cmd, cwd=build_dir, silent=True)

    log_sub("Compiling Daemon...")
    run_cmd(["ninja"], cwd=build_dir)

    binary = build_dir / "qos_daemon"

    if not binary.exists():
        log_err("Daemon build failed: binary not found.")

    log_ok("Daemon build successful")

    return binary


def extract_version(root_dir):
    prop_file = root_dir / "magisk-module" / "module.prop"

    if not prop_file.exists():
        log_err("module.prop not found.")

    content = prop_file.read_text()

    match = re.search(r"^version=(.+)$", content, re.MULTILINE)

    if not match:
        log_err("Could not parse version from module.prop.")

    return match.group(1).strip()


def package_module(root_dir, daemon_bin, version):
    log_info("Packaging Magisk Module...")

    magisk_dir = root_dir / "magisk-module"

    daemon_dest = magisk_dir / "system" / "bin" / "qos_daemon"

    daemon_dest.parent.mkdir(parents=True, exist_ok=True)

    shutil.copy2(daemon_bin, daemon_dest)
    log_sub(f"Placed Daemon at {daemon_dest.relative_to(root_dir)}")

    output_dir = root_dir / "scripts" / "output"
    output_dir.mkdir(parents=True, exist_ok=True)

    zip_name = f"qos-v{version}"
    zip_base_path = output_dir / zip_name

    log_sub("Zipping module contents...")

    shutil.make_archive(
        base_name=str(zip_base_path),
        format="zip",
        root_dir=str(magisk_dir),
    )

    final_zip = output_dir / f"{zip_name}.zip"

    if final_zip.exists():
        log_ok(
            f"Module packaged successfully: "
            f"{Style.BOLD}{final_zip.relative_to(root_dir)}{Style.RESET}"
        )

    else:
        log_err("Packaging failed: zip not found.")


def main():
    root_dir = Path(__file__).resolve().parent.parent
    os.chdir(root_dir)

    parser = argparse.ArgumentParser(
        formatter_class=lambda prog: argparse.ArgumentDefaultsHelpFormatter(
            prog,
            max_help_position=45,
            width=100,
        )
    )

    parser.add_argument(
        "--target",
        choices=["daemon", "module", "all", "none"],
        default="all",
        help="Build target",
    )

    parser.add_argument(
        "--api",
        default=DEFAULT_ANDROID_API,
        help="Android API level",
    )

    parser.add_argument(
        "--type",
        choices=["Release", "Debug"],
        default=DEFAULT_BUILD_TYPE,
        help="Build type",
    )

    parser.add_argument(
        "--skip-clean",
        action="store_true",
        help="Skip workspace cleaning",
    )

    parser.add_argument(
        "--skip-check",
        action="store_true",
        help="Skip syntax checks",
    )

    parser.add_argument(
        "--skip-lint",
        action="store_true",
        help="Skip linter",
    )

    args = parser.parse_args()

    if not args.skip_clean:
        log_info("Cleaning workspace...")
        clean_workspace(root_dir)

    do_check = not args.skip_check
    do_lint = not args.skip_lint

    start = time.time()

    try:
        needs_native_tools = (
            do_check
            or do_lint
            or args.target in ["daemon", "module", "all"]
        )

        ndk_path = None

        if needs_native_tools:
            for tool in ["cmake", "ninja", "rustup", "cargo"]:
                check_tool(tool)

            ndk_path = find_ndk()

            if not ndk_path:
                log_err("Android NDK not found. Set ANDROID_NDK_HOME.")

            run_quality_checks(
                ndk_path,
                args.api,
                root_dir,
                do_check,
                do_lint,
            )

        daemon_bin = None

        if args.target in ["daemon", "module", "all"]:
            daemon_bin = build_daemon(
                ndk_path,
                args.api,
                args.type,
                root_dir,
            )

        if args.target in ["module", "all"]:
            if not daemon_bin:
                log_err(
                    "Cannot package module: "
                    "Missing Daemon artifact."
                )

            version = extract_version(root_dir)

            package_module(
                root_dir,
                daemon_bin,
                version,
            )

        elapsed = time.time() - start

        print(
            f"\n{Style.GREEN}"
            f"Process finished in {elapsed:.2f}s"
            f"{Style.RESET}"
        )

    except KeyboardInterrupt:
        print("\nCancelled.")
        sys.exit(0)

    except Exception as exc:
        log_err(f"Unexpected error: {exc}")


if __name__ == "__main__":
    main()
