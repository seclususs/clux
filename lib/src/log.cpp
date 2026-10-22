// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "log.hpp"

#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

#include <sys/stat.h>

#if defined(__ANDROID__)
#include <android/log.h>
#endif

namespace
{

constexpr const char *TAG = "cluxd";
constexpr const char *BACKUP_SUFFIX = ".1";
constexpr uint32_t LEVEL_MIN = 3;
constexpr uint32_t LEVEL_MAX = 6;
constexpr size_t LINE_CAP = 192;
constexpr size_t ROW_CAP = 256;
constexpr size_t PATH_CAP = 64;
constexpr size_t BACKUP_CAP = PATH_CAP + 4;
constexpr uint32_t SINK_MIN = 4096;
constexpr uint32_t SINK_MAX = 1048576;
constexpr mode_t SINK_MODE = 0600;
constexpr char LEVEL_MARKS[] = {'D', 'I', 'W', 'E'};
constexpr int64_t SECONDS_PER_DAY = 86400;

struct Sink {
    int fd;
    size_t used;
    size_t cap;
    char path[PATH_CAP];
    char backup[BACKUP_CAP];
};

struct Stamp {
    int64_t year;
    uint32_t month;
    uint32_t day;
    uint32_t hour;
    uint32_t minute;
    uint32_t second;
    uint32_t milli;
};

Sink sink = {.fd = -1, .used = 0, .cap = 0, .path = {}, .backup = {}};

uint32_t bounded(uint32_t level) noexcept
{
    if (level < LEVEL_MIN) {
        return LEVEL_MIN;
    }

    return level > LEVEL_MAX ? LEVEL_MAX : level;
}

Stamp split(const timespec &now) noexcept
{
    int64_t days = now.tv_sec / SECONDS_PER_DAY;
    int64_t rest = now.tv_sec % SECONDS_PER_DAY;

    if (rest < 0) {
        rest += SECONDS_PER_DAY;
        --days;
    }

    const int64_t shifted = days + 719468;
    const int64_t era = (shifted >= 0 ? shifted : shifted - 146096) / 146097;

    const int64_t day_of_era = shifted - (era * 146097);

    const int64_t year_of_era =
        (day_of_era - (day_of_era / 1460) + (day_of_era / 36524) - (day_of_era / 146096)) / 365;

    const int64_t day_of_year =
        day_of_era - ((365 * year_of_era) + (year_of_era / 4) - (year_of_era / 100));

    const int64_t month_index = ((5 * day_of_year) + 2) / 153;
    const int64_t month = month_index < 10 ? month_index + 3 : month_index - 9;

    Stamp out{};
    out.year = year_of_era + (era * 400) + (month <= 2 ? 1 : 0);
    out.month = static_cast<uint32_t>(month);
    out.day = static_cast<uint32_t>(day_of_year - (((153 * month_index) + 2) / 5) + 1);
    out.hour = static_cast<uint32_t>(rest / 3600);
    out.minute = static_cast<uint32_t>(rest % 3600 / 60);
    out.second = static_cast<uint32_t>(rest % 60);
    out.milli = static_cast<uint32_t>(now.tv_nsec / 1000000);
    return out;
}

size_t format(char *row, size_t cap, const timespec &now, uint32_t level, const char *text) noexcept
{
    const Stamp at = split(now);
    const int len =
        ::snprintf(row, cap, "%04lld-%02u-%02u %02u:%02u:%02u.%03u %c %s\n",
                   static_cast<long long>(at.year), at.month, at.day, at.hour, at.minute, at.second,
                   at.milli, LEVEL_MARKS[level - LEVEL_MIN], text);
    if (len <= 0) {
        return 0;
    }

    auto size = static_cast<size_t>(len);
    if (size >= cap) {
        size = cap - 1;
        row[size - 1] = '\n';
    }

    return size;
}

int32_t claim(const char *path, int extra, size_t &size) noexcept
{
    const int flags = O_WRONLY | O_APPEND | O_CREAT | O_CLOEXEC | O_NOFOLLOW | extra;
    clux::Fd fd(clux::retry([&] { return ::open(path, flags, SINK_MODE); }));
    if (!fd.valid()) {
        return clux::fail();
    }

    struct stat st{};
    if (::fstat(fd.get(), &st) != 0) {
        return clux::fail();
    }

    if (!clux::trusted(st)) {
        return -EPERM;
    }

    size = static_cast<size_t>(st.st_size);
    return fd.release();
}

int32_t reopen(const char *path, size_t &size) noexcept
{
    const int32_t first = claim(path, 0, size);
    if (first != -EPERM && first != -ELOOP) {
        return first;
    }

    if (::unlink(path) != 0 && errno != ENOENT) {
        return clux::fail();
    }

    return claim(path, O_EXCL, size);
}

void rotate() noexcept
{
    (void)::close(sink.fd);
    sink.fd = -1;
    sink.used = 0;

    if (::rename(sink.path, sink.backup) != 0 && ::unlink(sink.path) != 0) {
        return;
    }

    const int32_t fd = reopen(sink.path, sink.used);
    sink.fd = fd < 0 ? -1 : fd;
}

void append(uint32_t level, const char *text) noexcept
{
    timespec now{};
    if (sink.fd < 0 || ::clock_gettime(CLOCK_REALTIME, &now) != 0) {
        return;
    }

    char row[ROW_CAP];
    const size_t size = format(row, sizeof(row), now, level, text);
    if (size == 0) {
        return;
    }

    if (sink.used + size > sink.cap) {
        rotate();
    }

    if (sink.fd < 0) {
        return;
    }

    const ssize_t done = clux::retry([&] { return ::write(sink.fd, row, size); });
    if (done > 0) {
        sink.used += static_cast<size_t>(done);
    }
}

void emit(uint32_t level, const char *text) noexcept
{
    const uint32_t bound = bounded(level);
#ifdef __ANDROID__
    (void)::__android_log_write(static_cast<int>(bound), TAG, text);
#else
    (void)::fprintf(stderr, "%s[%u] %s\n", TAG, bound, text);
#endif
    append(bound, text);
}

}

CLUX_API int32_t clux_log_open(const char *path, uint32_t cap) noexcept
{
    const size_t len = path == nullptr ? 0 : ::strnlen(path, PATH_CAP);
    if (len == 0 || len >= PATH_CAP || cap < SINK_MIN || cap > SINK_MAX) {
        return -EINVAL;
    }

    size_t size = 0;
    const int32_t fd = reopen(path, size);
    if (fd < 0) {
        return fd;
    }

    if (sink.fd >= 0) {
        (void)::close(sink.fd);
    }

    sink.fd = fd;
    sink.used = size;
    sink.cap = cap;
    ::memcpy(sink.path, path, len + 1);
    (void)::snprintf(sink.backup, sizeof(sink.backup), "%s%s", path, BACKUP_SUFFIX);
    return 0;
}

CLUX_API void clux_log(uint32_t level, const char *msg) noexcept
{
    if (msg != nullptr) {
        emit(level, msg);
    }
}

CLUX_API void clux_log_num(uint32_t level, const char *msg, int64_t value) noexcept
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
