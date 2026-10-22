// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

CLUX_API int32_t clux_log_open(const char *path, uint32_t cap) noexcept;
CLUX_API void clux_log(uint32_t level, const char *msg) noexcept;
CLUX_API void clux_log_num(uint32_t level, const char *msg, int64_t value) noexcept;
