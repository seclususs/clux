// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "node.hpp"

#include <fcntl.h>
#include <sched.h>
#include <string.h>

#include <linux/magic.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/sysmacros.h>
#include <sys/vfs.h>

namespace clux::node
{

namespace
{

constexpr size_t DIRENT_RECLEN = 16;
constexpr size_t DIRENT_NAME = 19;
constexpr const char *DEBUGFS_ROOT = "/sys/kernel/debug";

int access_flags(uint32_t mode) noexcept
{
    switch (mode) {
    case MODE_READ:
        return O_RDONLY;

    case MODE_WRITE:
        return O_WRONLY;

    case MODE_RDWR:
        return O_RDWR;

    default:
        return -1;
    }
}

bool dot(const char *name, size_t len) noexcept
{
    return (len == 1 && name[0] == '.') || (len == 2 && name[0] == '.' && name[1] == '.');
}

}

int32_t pack(Packer &packer, const char *chunk, size_t bytes) noexcept
{
    size_t pos = 0;

    while (pos < bytes) {
        uint16_t reclen = 0;

        ::memcpy(&reclen, chunk + pos + DIRENT_RECLEN, sizeof(reclen));
        if (reclen <= DIRENT_NAME || pos + reclen > bytes) {
            return -EIO;
        }

        const char *name = chunk + pos + DIRENT_NAME;
        const size_t len = ::strnlen(name, reclen - DIRENT_NAME);

        pos += reclen;

        if (dot(name, len)) {
            continue;
        }

        if (packer.used + len + 1 > packer.cap) {
            return -ENOSPC;
        }

        ::memcpy(packer.out + packer.used, name, len);
        packer.out[packer.used + len] = '\0';
        packer.used += len + 1;
    }

    return 0;
}

bool pseudo(int fd) noexcept
{
    struct statfs info{};

    if (::fstatfs(fd, &info) != 0) {
        return false;
    }

    using Magic = decltype(info.f_type);
    const Magic type = info.f_type;

    return type == static_cast<Magic>(PROC_SUPER_MAGIC) ||
           type == static_cast<Magic>(SYSFS_MAGIC) || type == static_cast<Magic>(DEBUGFS_MAGIC);
}

int32_t put(const char *path, const char *text) noexcept
{
    const int32_t raw = clux_node_open(path, MODE_WRITE);
    if (raw < 0) {
        return raw;
    }

    const Fd fd(raw);
    const size_t len = ::strlen(text);
    const ssize_t done = retry([&] { return ::write(fd.get(), text, len); });
    return done < 0 ? fail() : 0;
}

}

CLUX_API int32_t clux_node_open(const char *path, uint32_t mode)
{
    const int access = clux::node::access_flags(mode);
    if (path == nullptr || access < 0) {
        return -EINVAL;
    }

    const int flags = access | O_CLOEXEC | O_NOFOLLOW;
    clux::Fd fd(clux::retry([&] { return ::open(path, flags); }));
    if (!fd.valid()) {
        return clux::fail();
    }

    if (!clux::node::pseudo(fd.get())) {
        return -EPERM;
    }

    return fd.release();
}

CLUX_API int32_t clux_node_read(int32_t fd, void *buf, size_t cap)
{
    if (fd < 0 || buf == nullptr || cap > clux::IO_LIMIT) {
        return -EINVAL;
    }

    const ssize_t got = clux::retry([&] { return ::pread(fd, buf, cap, 0); });
    return got < 0 ? clux::fail() : static_cast<int32_t>(got);
}

CLUX_API int32_t clux_node_write(int32_t fd, const void *buf, size_t len)
{
    if (fd < 0 || buf == nullptr || len > clux::IO_LIMIT) {
        return -EINVAL;
    }

    const ssize_t done = clux::retry([&] { return ::pwrite(fd, buf, len, 0); });
    return done < 0 ? clux::fail() : static_cast<int32_t>(done);
}

CLUX_API void clux_node_close(int32_t fd)
{
    if (fd >= 0) {
        (void)::close(fd);
    }
}

CLUX_API int32_t clux_node_slurp(const char *path, void *buf, size_t cap)
{
    const int32_t fd = clux_node_open(path, clux::node::MODE_READ);
    if (fd < 0) {
        return fd;
    }

    const int32_t got = clux_node_read(fd, buf, cap);
    clux_node_close(fd);
    return got;
}

CLUX_API int32_t clux_node_scan(const char *dir, void *buf, size_t cap)
{
    if (dir == nullptr || buf == nullptr || cap == 0 || cap > clux::IO_LIMIT) {
        return -EINVAL;
    }

    const int flags = O_RDONLY | O_DIRECTORY | O_CLOEXEC;
    const clux::Fd fd(clux::retry([&] { return ::open(dir, flags); }));
    if (!fd.valid()) {
        return clux::fail();
    }

    if (!clux::node::pseudo(fd.get())) {
        return -EPERM;
    }

    alignas(8) char chunk[clux::node::SCAN_CHUNK];
    clux::node::Packer packer{.out = static_cast<char *>(buf), .cap = cap, .used = 0};

    for (;;) {
        const long got =
            ::syscall(SYS_getdents64, static_cast<long>(fd.get()), chunk, sizeof(chunk));

        if (got < 0 && errno == EINTR) {
            continue;
        }

        if (got < 0) {
            return clux::fail();
        }

        if (got == 0) {
            return static_cast<int32_t>(packer.used);
        }

        const int32_t packed = clux::node::pack(packer, chunk, static_cast<size_t>(got));
        if (packed < 0) {
            return packed;
        }
    }
}

CLUX_API int32_t clux_node_devno(const char *path, uint32_t *major_out, uint32_t *minor_out)
{
    if (path == nullptr || major_out == nullptr || minor_out == nullptr) {
        return -EINVAL;
    }

    struct stat st{};
    if (clux::retry([&] { return ::stat(path, &st); }) < 0) {
        return clux::fail();
    }

    const auto major_id = static_cast<uint32_t>(major(st.st_dev));
    if (major_id == 0) {
        return -ENODEV;
    }

    *major_out = major_id;
    *minor_out = static_cast<uint32_t>(minor(st.st_dev));
    return 0;
}

CLUX_API int32_t clux_node_debugfs(void)
{
    if (::unshare(CLONE_NEWNS) != 0) {
        return clux::fail();
    }

    if (::mount(nullptr, "/", nullptr, MS_REC | MS_PRIVATE, nullptr) != 0) {
        return clux::fail();
    }

    const unsigned long flags = MS_NOSUID | MS_NODEV | MS_NOEXEC;
    if (::mount("debugfs", clux::node::DEBUGFS_ROOT, "debugfs", flags, nullptr) != 0) {
        return clux::fail();
    }

    return 0;
}
