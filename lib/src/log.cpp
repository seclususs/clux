// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "log.hpp"

#include <stdio.h>

#if defined(__ANDROID__)
#include <android/log.h>
#endif

namespace
{

constexpr const char *TAG = "cluxd";
constexpr uint32_t LEVEL_MIN = 3;
constexpr uint32_t LEVEL_MAX = 6;
constexpr size_t LINE_CAP = 192;

uint32_t bounded(uint32_t level) noexcept
{
    if (level < LEVEL_MIN) {
        return LEVEL_MIN;
    }

    return level > LEVEL_MAX ? LEVEL_MAX : level;
}

void emit(uint32_t level, const char *text) noexcept
{
#ifdef __ANDROID__
    (void)::__android_log_write(static_cast<int>(bounded(level)), TAG, text);
#else
    (void)::fprintf(stderr, "%s[%u] %s\n", TAG, bounded(level), text);
#endif
}

}

CLUX_API void clux_log(uint32_t level, const char *msg)
{
    if (msg != nullptr) {
        emit(level, msg);
    }
}

CLUX_API void clux_log_num(uint32_t level, const char *msg, int64_t value)
{
    if (msg == nullptr) {
        return;
    }

    char line[LINE_CAP];
    const int len = ::snprintf(line, sizeof(line), "%s: %lld", msg, static_cast<long long>(value));
    if (len > 0) {
        emit(level, line);
    }
}
