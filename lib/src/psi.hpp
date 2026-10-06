// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

namespace clux::psi
{

constexpr uint32_t WINDOW_MIN_US = 500000;
constexpr uint32_t WINDOW_MAX_US = 10000000;

}

CLUX_API int32_t clux_psi_arm(const char *path, uint32_t threshold_us, uint32_t window_us);
