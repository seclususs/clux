// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

CLUX_API int32_t clux_proc_root(void) noexcept;
CLUX_API int32_t clux_proc_detach(void) noexcept;
CLUX_API int32_t clux_proc_lock(const char *path) noexcept;
CLUX_API int32_t clux_proc_journal(const char *node, uint32_t origin) noexcept;
CLUX_API int32_t clux_proc_sentinel(void) noexcept;
CLUX_API void clux_proc_settle(void) noexcept;
CLUX_API int32_t clux_proc_shield(void) noexcept;
CLUX_API int32_t clux_proc_limit(void) noexcept;
CLUX_API int32_t clux_proc_cores(void) noexcept;
CLUX_API int32_t clux_proc_pin(uint64_t mask) noexcept;
CLUX_API int32_t clux_proc_realtime(uint32_t priority) noexcept;
CLUX_API int32_t clux_proc_clamp(uint32_t util_max) noexcept;
CLUX_API int32_t clux_proc_ioprio(void) noexcept;
CLUX_API int32_t clux_proc_slack(uint64_t nanos) noexcept;
CLUX_API int32_t clux_proc_lockmem(void) noexcept;
CLUX_API __attribute__((noreturn)) void clux_proc_abort(void) noexcept;
