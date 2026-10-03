// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include "base.hpp"

namespace clux::node
{

constexpr uint32_t MODE_READ = 0;
constexpr uint32_t MODE_WRITE = 1;
constexpr uint32_t MODE_RDWR = 2;
constexpr size_t SCAN_CHUNK = 2048;

struct Packer {
    char *out;
    size_t cap;
    size_t used;
};

[[nodiscard]] bool pseudo(int fd) noexcept;
[[nodiscard]] int32_t put(const char *path, const char *text) noexcept;
[[nodiscard]] int32_t pack(Packer &packer, const char *chunk, size_t bytes) noexcept;

}

CLUX_API int32_t clux_node_open(const char *path, uint32_t mode);
CLUX_API int32_t clux_node_read(int32_t fd, void *buf, size_t cap);
CLUX_API int32_t clux_node_write(int32_t fd, const void *buf, size_t len);
CLUX_API void clux_node_close(int32_t fd);
CLUX_API int32_t clux_node_slurp(const char *path, void *buf, size_t cap);
CLUX_API int32_t clux_node_scan(const char *dir, void *buf, size_t cap);
