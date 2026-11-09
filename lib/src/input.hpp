// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

namespace clux::input
{

constexpr uint32_t CAP_POINTER = 1;
constexpr uint32_t CAP_DIRECT = 2;
constexpr uint32_t CAP_MULTITOUCH = 4;
constexpr uint32_t CAP_ABSOLUTE = 8;
constexpr uint32_t CAP_CONTACT = 16;

}

CLUX_API int32_t clux_input_open(const char *path) noexcept;
CLUX_API int32_t clux_input_caps(int32_t fd) noexcept;
CLUX_API int32_t clux_input_drain(int32_t fd) noexcept;
