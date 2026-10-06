// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "prop.hpp"

#include <string.h>

#if defined(__ANDROID__)
#include <sys/system_properties.h>
#endif

#if defined(__ANDROID__)

namespace
{

struct Sink {
    char *out;
    size_t cap;
    int32_t len;
};

void collect(void *cookie, [[maybe_unused]] const char *name, const char *value,
             [[maybe_unused]] uint32_t serial) noexcept
{
    auto *sink = static_cast<Sink *>(cookie);

    const size_t len = ::strnlen(value, PROP_VALUE_MAX);
    const size_t take = len < sink->cap ? len : sink->cap - 1;
    ::memcpy(sink->out, value, take);

    sink->out[take] = '\0';
    sink->len = static_cast<int32_t>(take);
}

}

CLUX_API int32_t clux_prop_get(const char *key, char *out, size_t cap)
{
    if (key == nullptr || out == nullptr || cap == 0) {
        return -EINVAL;
    }

    out[0] = '\0';

    const prop_info *info = ::__system_property_find(key);
    if (info == nullptr) {
        return -ENOENT;
    }

    Sink sink{.out = out, .cap = cap, .len = 0};
    ::__system_property_read_callback(info, collect, &sink);
    return sink.len;
}

#else

CLUX_API int32_t clux_prop_get(const char *key, char *out, size_t cap)
{
    if (key == nullptr || out == nullptr || cap < 2) {
        return -EINVAL;
    }

    if (::strcmp(key, "sys.boot_completed") != 0) {
        return -ENOENT;
    }

    out[0] = '1';
    out[1] = '\0';

    return 1;
}

#endif
