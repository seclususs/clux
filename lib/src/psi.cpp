// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "psi.hpp"

#include "node.hpp"

#include <fcntl.h>
#include <stdio.h>
#include <string.h>

namespace
{

constexpr char PSI_PREFIX[] = "/proc/pressure/";
constexpr size_t TRIGGER_CAP = 32;

}

CLUX_API int32_t clux_psi_arm(const char *path, uint32_t threshold_us, uint32_t window_us)
{
    using namespace clux::psi;

    if (path == nullptr || ::strncmp(path, PSI_PREFIX, sizeof(PSI_PREFIX) - 1) != 0) {
        return -EINVAL;
    }

    if (window_us < WINDOW_MIN_US || window_us > WINDOW_MAX_US || threshold_us == 0 ||
        threshold_us > window_us) {
        return -EINVAL;
    }

    char text[TRIGGER_CAP];

    const int len = ::snprintf(text, sizeof(text), "some %u %u\n", threshold_us, window_us);
    if (len <= 0 || static_cast<size_t>(len) >= sizeof(text)) {
        return -EOVERFLOW;
    }

    const int flags = O_RDWR | O_NONBLOCK | O_CLOEXEC;
    clux::Fd fd(clux::retry([&] { return ::open(path, flags); }));
    if (!fd.valid()) {
        return clux::fail();
    }

    if (!clux::node::pseudo(fd.get())) {
        return -EPERM;
    }

    const ssize_t done =
        clux::retry([&] { return ::write(fd.get(), text, static_cast<size_t>(len)); });

    if (done != len) {
        return done < 0 ? clux::fail() : -EIO;
    }

    return fd.release();
}
