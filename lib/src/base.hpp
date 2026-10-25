// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <unistd.h>

#include <sys/stat.h>

#define CLUX_API extern "C" __attribute__((visibility("default")))

namespace clux
{

constexpr uint32_t ABI_VERSION = 3;
constexpr size_t IO_LIMIT = 65536;
constexpr mode_t WRITABLE_BY_OTHERS = 022;

[[nodiscard]] inline int32_t fail() noexcept
{
    return -errno;
}

[[nodiscard]] inline bool trusted(const struct stat &st) noexcept
{
    return S_ISREG(st.st_mode) && st.st_uid == 0 && st.st_nlink == 1 &&
           (st.st_mode & WRITABLE_BY_OTHERS) == 0;
}

[[nodiscard]] inline bool replaceable(int32_t err) noexcept
{
    return err == -EPERM || err == -ELOOP || err == -ENXIO;
}

class Fd
{
  public:
    explicit Fd(int fd) noexcept : fd_(fd) {}
    Fd(const Fd &) = delete;
    Fd &operator=(const Fd &) = delete;
    Fd(Fd &&) = delete;
    Fd &operator=(Fd &&) = delete;

    ~Fd()
    {
        if (fd_ >= 0) {
            (void)::close(fd_);
        }
    }

    [[nodiscard]] int get() const noexcept { return fd_; }
    [[nodiscard]] bool valid() const noexcept { return fd_ >= 0; }
    [[nodiscard]] int release() noexcept
    {
        const int fd = fd_;
        fd_ = -1;
        return fd;
    }

  private:
    int fd_;
};

template <typename Op>
[[nodiscard]] inline auto retry(Op op) noexcept
{
    auto result = op();
    while (result < 0 && errno == EINTR) {
        result = op();
    }
    return result;
}

}

CLUX_API uint32_t clux_abi(void) noexcept;
