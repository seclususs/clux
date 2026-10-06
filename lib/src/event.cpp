// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "event.hpp"

#include <pthread.h>
#include <signal.h>
#include <time.h>

#include <sys/epoll.h>
#include <sys/signalfd.h>

namespace
{

constexpr uint64_t MICROS = 1000000;
constexpr uint64_t NANOS_PER_MICRO = 1000;
constexpr uint32_t SIGNAL_BATCH = 4;

uint32_t to_epoll(uint32_t flags) noexcept
{
    uint32_t raw = 0;

    if ((flags & clux::event::FLAG_READ) != 0) {
        raw |= EPOLLIN;
    }

    if ((flags & clux::event::FLAG_PRIORITY) != 0) {
        raw |= EPOLLPRI;
    }

    return raw | EPOLLERR;
}

uint32_t from_epoll(uint32_t raw) noexcept
{
    uint32_t flags = 0;

    if ((raw & EPOLLIN) != 0) {
        flags |= clux::event::FLAG_READ;
    }

    if ((raw & EPOLLPRI) != 0) {
        flags |= clux::event::FLAG_PRIORITY;
    }

    if ((raw & (EPOLLERR | EPOLLHUP)) != 0) {
        flags |= clux::event::FLAG_FAULT;
    }

    return flags;
}

}

CLUX_API int32_t clux_epoll_open(void)
{
    const int fd = ::epoll_create1(EPOLL_CLOEXEC);
    return fd < 0 ? clux::fail() : fd;
}

CLUX_API int32_t clux_epoll_add(int32_t ep, int32_t fd, uint64_t token, uint32_t flags)
{
    epoll_event ev{};
    ev.events = to_epoll(flags);
    ev.data.u64 = token;
    return ::epoll_ctl(ep, EPOLL_CTL_ADD, fd, &ev) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_epoll_del(int32_t ep, int32_t fd)
{
    return ::epoll_ctl(ep, EPOLL_CTL_DEL, fd, nullptr) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_epoll_wait(int32_t ep, clux::event::Ready *out, uint32_t cap,
                                 int32_t timeout_ms)
{
    if (out == nullptr || cap == 0) {
        return -EINVAL;
    }

    epoll_event raw[clux::event::BATCH] = {};
    const uint32_t want = cap < clux::event::BATCH ? cap : clux::event::BATCH;

    const int got = ::epoll_wait(ep, raw, static_cast<int>(want), timeout_ms);
    if (got < 0) {
        return errno == EINTR ? 0 : clux::fail();
    }

    for (int i = 0; i < got; ++i) {
        out[i] = {.token = raw[i].data.u64, .flags = from_epoll(raw[i].events), .reserved = 0};
    }

    return got;
}

CLUX_API int32_t clux_signal_open(void)
{
    sigset_t set;

    if (::sigemptyset(&set) != 0 || ::sigaddset(&set, SIGINT) != 0 ||
        ::sigaddset(&set, SIGTERM) != 0 || ::sigaddset(&set, SIGHUP) != 0) {
        return clux::fail();
    }

    const int blocked = ::pthread_sigmask(SIG_BLOCK, &set, nullptr);
    if (blocked != 0) {
        return -blocked;
    }

    const int fd = ::signalfd(-1, &set, SFD_CLOEXEC | SFD_NONBLOCK);
    return fd < 0 ? clux::fail() : fd;
}

CLUX_API int32_t clux_signal_drain(int32_t fd)
{
    signalfd_siginfo info[SIGNAL_BATCH];
    int32_t total = 0;

    for (;;) {
        const ssize_t got = ::read(fd, info, sizeof(info));

        if (got < 0 && errno == EINTR) {
            continue;
        }

        if (got < 0 && errno == EAGAIN) {
            break;
        }

        if (got < 0) {
            return clux::fail();
        }

        if (got == 0) {
            break;
        }

        total += static_cast<int32_t>(static_cast<size_t>(got) / sizeof(signalfd_siginfo));
    }

    return total;
}

CLUX_API uint64_t clux_clock_us(void)
{
    timespec ts{};

    if (::clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }

    return (static_cast<uint64_t>(ts.tv_sec) * MICROS) +
           (static_cast<uint64_t>(ts.tv_nsec) / NANOS_PER_MICRO);
}
