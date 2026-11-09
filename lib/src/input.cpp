// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "input.hpp"

#include <fcntl.h>

#include <linux/input.h>
#include <sys/syscall.h>

namespace
{

constexpr size_t KEY_BYTES = (KEY_MAX / 8) + 1;
constexpr size_t ABS_BYTES = (ABS_MAX / 8) + 1;
constexpr size_t PROP_BYTES = (INPUT_PROP_MAX / 8) + 1;
constexpr size_t DRAIN_BATCH = 32;
constexpr size_t DRAIN_ROUNDS = 8;

bool query(int fd, unsigned long request, void *out) noexcept
{
    return ::syscall(SYS_ioctl, static_cast<long>(fd), request, out) >= 0;
}

template <size_t N>
bool has_bit(const uint8_t (&bits)[N], unsigned int code) noexcept
{
    const size_t byte = code / 8;
    return byte < N && ((bits[byte] >> (code % 8)) & 1U) != 0;
}

uint32_t flag(bool set, uint32_t bit) noexcept
{
    return set ? bit : 0;
}

}

CLUX_API int32_t clux_input_open(const char *path) noexcept
{
    if (path == nullptr) {
        return -EINVAL;
    }

    const int flags = O_RDONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW;
    clux::Fd fd(clux::retry([&] { return ::open(path, flags); }));
    if (!fd.valid()) {
        return clux::fail();
    }

    int version = 0;
    if (!query(fd.get(), EVIOCGVERSION, &version)) {
        return clux::fail();
    }

    return fd.release();
}

CLUX_API int32_t clux_input_caps(int32_t fd) noexcept
{
    uint8_t keys[KEY_BYTES] = {};
    uint8_t axes[ABS_BYTES] = {};
    uint8_t props[PROP_BYTES] = {};

    if (!query(fd, EVIOCGBIT(EV_KEY, sizeof(keys)), keys) ||
        !query(fd, EVIOCGBIT(EV_ABS, sizeof(axes)), axes)) {
        return clux::fail();
    }

    (void)query(fd, EVIOCGPROP(sizeof(props)), props);

    const bool multitouch = has_bit(axes, ABS_MT_POSITION_X) && has_bit(axes, ABS_MT_POSITION_Y);
    const bool absolute = has_bit(axes, ABS_X) && has_bit(axes, ABS_Y);

    return static_cast<int32_t>(flag(has_bit(props, INPUT_PROP_POINTER), clux::input::CAP_POINTER) |
                                flag(has_bit(props, INPUT_PROP_DIRECT), clux::input::CAP_DIRECT) |
                                flag(multitouch, clux::input::CAP_MULTITOUCH) |
                                flag(absolute, clux::input::CAP_ABSOLUTE) |
                                flag(has_bit(keys, BTN_TOUCH), clux::input::CAP_CONTACT));
}

CLUX_API int32_t clux_input_drain(int32_t fd) noexcept
{
    input_event events[DRAIN_BATCH];
    int32_t total = 0;

    for (size_t round = 0; round < DRAIN_ROUNDS; ++round) {
        const ssize_t got = ::read(fd, events, sizeof(events));

        if (got < 0 && errno == EINTR) {
            continue;
        }

        if (got < 0 && errno == EAGAIN) {
            break;
        }

        if (got < 0) {
            return clux::fail();
        }

        total += static_cast<int32_t>(static_cast<size_t>(got) / sizeof(input_event));

        if (static_cast<size_t>(got) < sizeof(events)) {
            break;
        }
    }

    return total;
}
