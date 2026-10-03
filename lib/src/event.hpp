// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

namespace clux::event
{

constexpr uint32_t FLAG_READ = 1;
constexpr uint32_t FLAG_PRIORITY = 2;
constexpr uint32_t FLAG_FAULT = 4;
constexpr uint32_t BATCH = 16;

struct Ready {
    uint64_t token;
    uint32_t flags;
    uint32_t reserved;
};

static_assert(sizeof(Ready) == 16);

}

CLUX_API int32_t clux_epoll_open(void);
CLUX_API int32_t clux_epoll_add(int32_t ep, int32_t fd, uint64_t token, uint32_t flags);
CLUX_API int32_t clux_epoll_del(int32_t ep, int32_t fd);
CLUX_API int32_t clux_epoll_wait(int32_t ep, clux::event::Ready *out, uint32_t cap,
                                 int32_t timeout_ms);

CLUX_API int32_t clux_signal_open(void);
CLUX_API int32_t clux_signal_drain(int32_t fd);

CLUX_API uint64_t clux_clock_us(void);
