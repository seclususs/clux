// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "proc.hpp"

#include "log.hpp"
#include "node.hpp"

#include <fcntl.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <sys/file.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/syscall.h>

namespace
{

constexpr mode_t LOCK_MODE = 0600;
constexpr mode_t DAEMON_UMASK = 077;
constexpr int STD_FDS = 3;
constexpr size_t LEDGER_CAP = 2048;
constexpr size_t NODE_CAP = 96;
constexpr size_t BOOT_CAP = 48;
constexpr size_t VALUE_CAP = 16;
constexpr const char *BOOT_NODE = "/proc/sys/kernel/random/boot_id";
constexpr const char *BOOT_UNKNOWN = "-";
constexpr const char *OOM_NODE = "/proc/self/oom_score_adj";
constexpr const char *OOM_SHIELD = "-1000\n";
constexpr const char *DEBUGFS_ROOT = "/sys/kernel/debug/";
constexpr const char *SENTINEL_NAME = "clux-sentinel";
constexpr uint32_t LOG_ERROR = 6;
constexpr rlim_t LIMIT_NOFILE = 64;
constexpr rlim_t LIMIT_STACK = 262144;
constexpr rlim_t LIMIT_RTTIME_US = 50000;
constexpr uint64_t CPU_MASK_BITS = 64;
constexpr long IOPRIO_WHO_PROCESS = 1;
constexpr long IOPRIO_CLASS_BEST_EFFORT = 2;
constexpr long IOPRIO_CLASS_SHIFT = 13;
constexpr uint64_t ATTR_KEEP_POLICY = 0x08;
constexpr uint64_t ATTR_KEEP_PARAMS = 0x10;
constexpr uint64_t ATTR_CLAMP_MIN = 0x20;
constexpr uint64_t ATTR_CLAMP_MAX = 0x40;

struct Attr {
    uint32_t size;
    uint32_t policy;
    uint64_t flags;
    int32_t nice;
    uint32_t priority;
    uint64_t runtime;
    uint64_t deadline;
    uint64_t period;
    uint32_t util_min;
    uint32_t util_max;
};

static_assert(sizeof(Attr) == 56);

using Resource = decltype(RLIMIT_CORE);

struct Limit {
    Resource resource;
    rlim_t cap;
};

struct Ledger {
    int fd;
    size_t used;
    char text[LEDGER_CAP];
};

struct Entry {
    uint32_t value;
    char node[NODE_CAP];
};

Ledger ledger = {.fd = -1, .used = 0, .text = {}};

const char *skip_line(const char *text) noexcept
{
    const char *end = ::strchr(text, '\n');
    return end == nullptr ? nullptr : end + 1;
}

const char *skip_header(const char *text) noexcept
{
    const char *first = skip_line(text);
    return first == nullptr ? nullptr : skip_line(first);
}

bool parse_entry(const char *&cursor, Entry &entry) noexcept
{
    uint64_t value = 0;
    const char *at = cursor;

    while (*at >= '0' && *at <= '9') {
        value = (value * 10) + static_cast<uint64_t>(*at - '0');
        if (value > UINT32_MAX) {
            return false;
        }

        ++at;
    }

    if (at == cursor || *at != ' ') {
        return false;
    }

    ++at;
    const char *end = ::strchr(at, '\n');
    const size_t len = end == nullptr ? 0 : static_cast<size_t>(end - at);
    if (len == 0 || len >= NODE_CAP) {
        return false;
    }

    ::memcpy(entry.node, at, len);
    entry.node[len] = '\0';
    entry.value = static_cast<uint32_t>(value);
    cursor = end + 1;
    return true;
}

bool known(const char *node, size_t len) noexcept
{
    const char *cursor = skip_header(ledger.text);
    Entry entry{};

    while (cursor != nullptr && *cursor != '\0' && parse_entry(cursor, entry)) {
        if (::strncmp(entry.node, node, len + 1) == 0) {
            return true;
        }
    }

    return false;
}

bool boot_id(char (&out)[BOOT_CAP]) noexcept
{
    const int32_t got = clux_node_slurp(BOOT_NODE, out, sizeof(out) - 1);
    if (got <= 0) {
        return false;
    }

    auto len = static_cast<size_t>(got);
    while (len > 0 && (out[len - 1] == '\n' || out[len - 1] == '\r')) {
        --len;
    }

    out[len] = '\0';
    return len > 0;
}

uint32_t recover(const char *old, const char *boot) noexcept
{
    const char *cursor = skip_line(old);
    const size_t len = ::strlen(boot);
    uint32_t restored = 0;

    if (cursor == nullptr || ::strncmp(cursor, boot, len) != 0 || cursor[len] != '\n') {
        return restored;
    }

    cursor += len + 1;

    if (::strstr(cursor, DEBUGFS_ROOT) != nullptr) {
        (void)clux_node_debugfs();
    }

    Entry entry{};

    while (*cursor != '\0' && parse_entry(cursor, entry)) {
        char text[VALUE_CAP];

        if (::snprintf(text, sizeof(text), "%u\n", entry.value) > 0 &&
            clux::node::put(entry.node, text) == 0) {
            ++restored;
        }
    }

    return restored;
}

void announce(uint32_t restored) noexcept
{
    if (restored > 0) {
        clux_log_num(LOG_ERROR, "knobs restored after an unclean exit",
                     static_cast<int64_t>(restored));
    }
}

void reclaim() noexcept
{
    char boot[BOOT_CAP] = {};
    char old[LEDGER_CAP] = {};
    const ssize_t got = ::pread(ledger.fd, old, sizeof(old) - 1, 0);

    if (got > 0 && boot_id(boot)) {
        announce(recover(old, boot));
    }

    (void)::ftruncate(ledger.fd, 0);
}

[[noreturn]] void stand_guard(int watch) noexcept
{
    sigset_t blocked;
    (void)::sigfillset(&blocked);
    (void)::pthread_sigmask(SIG_SETMASK, &blocked, nullptr);
    (void)::prctl(PR_SET_NAME, reinterpret_cast<unsigned long>(SENTINEL_NAME));

    char byte = 0;
    for (;;) {
        const ssize_t got = ::read(watch, &byte, sizeof(byte));
        if (got == 0) {
            break;
        }

        if (got < 0 && errno != EINTR) {
            ::_exit(1);
        }
    }

    reclaim();
    ::_exit(0);
}

bool stamp(int fd, const char *boot) noexcept
{
    const int len = ::snprintf(ledger.text, sizeof(ledger.text), "%d\n%s\n",
                               static_cast<int>(::getpid()), boot);
    if (len <= 0 || static_cast<size_t>(len) >= sizeof(ledger.text)) {
        return false;
    }

    ledger.used = static_cast<size_t>(len);
    return ::pwrite(fd, ledger.text, ledger.used, 0) == len;
}

int32_t claim(const char *path, int extra) noexcept
{
    const int flags = O_RDWR | O_CREAT | O_CLOEXEC | O_NOFOLLOW | extra;
    clux::Fd fd(clux::retry([&] { return ::open(path, flags, LOCK_MODE); }));
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

    if (::flock(fd.get(), LOCK_EX | LOCK_NB) != 0) {
        return clux::fail();
    }

    char boot[BOOT_CAP] = {};
    char old[LEDGER_CAP] = {};
    const bool known_boot = boot_id(boot);
    const ssize_t got = ::pread(fd.get(), old, sizeof(old) - 1, 0);

    if (known_boot && got > 0) {
        announce(recover(old, boot));
    }

    if (::ftruncate(fd.get(), 0) != 0 || !stamp(fd.get(), known_boot ? boot : BOOT_UNKNOWN)) {
        return clux::fail();
    }

    ledger.fd = fd.get();
    return fd.release();
}

bool redirect_stdio() noexcept
{
    clux::Fd null(::open("/dev/null", O_RDWR));
    if (!null.valid()) {
        return false;
    }

    for (int target = 0; target < STD_FDS; ++target) {
        if (null.get() != target && ::dup2(null.get(), target) < 0) {
            return false;
        }
    }

    if (null.get() < STD_FDS) {
        (void)null.release();
    }

    return true;
}

}

CLUX_API int32_t clux_proc_root(void)
{
    return ::geteuid() == 0 ? 0 : -EPERM;
}

CLUX_API int32_t clux_proc_detach(void)
{
    pid_t pid = ::fork();
    if (pid < 0) {
        return clux::fail();
    }
    if (pid > 0) {
        ::_exit(0);
    }

    if (::setsid() < 0) {
        return clux::fail();
    }

    pid = ::fork();
    if (pid < 0) {
        return clux::fail();
    }
    if (pid > 0) {
        ::_exit(0);
    }

    (void)::umask(DAEMON_UMASK);

    if (::chdir("/") != 0 || !redirect_stdio()) {
        return clux::fail();
    }

    return 0;
}

CLUX_API int32_t clux_proc_lock(const char *path)
{
    if (path == nullptr) {
        return -EINVAL;
    }

    const int32_t first = claim(path, 0);
    if (first != -EPERM && first != -ELOOP) {
        return first;
    }

    if (::unlink(path) != 0 && errno != ENOENT) {
        return clux::fail();
    }

    return claim(path, O_EXCL);
}

CLUX_API int32_t clux_proc_journal(const char *node, uint32_t origin)
{
    const size_t len = node == nullptr ? 0 : ::strnlen(node, NODE_CAP);
    if (len == 0 || len >= NODE_CAP || ::strchr(node, '\n') != nullptr || ledger.fd < 0) {
        return -EINVAL;
    }

    if (known(node, len)) {
        return 0;
    }

    char line[NODE_CAP + VALUE_CAP];
    const int size = ::snprintf(line, sizeof(line), "%u %s\n", origin, node);
    if (size <= 0) {
        return -EINVAL;
    }

    const auto add = static_cast<size_t>(size);
    if (ledger.used + add >= sizeof(ledger.text)) {
        return -ENOSPC;
    }

    if (::pwrite(ledger.fd, line, add, static_cast<off_t>(ledger.used)) != size) {
        return clux::fail();
    }

    ::memcpy(ledger.text + ledger.used, line, add);
    ledger.used += add;
    ledger.text[ledger.used] = '\0';
    return 0;
}

CLUX_API int32_t clux_proc_sentinel(void)
{
    if (ledger.fd < 0) {
        return -EINVAL;
    }

    int ends[2] = {-1, -1};
    if (::pipe2(ends, O_CLOEXEC) != 0) {
        return clux::fail();
    }

    const pid_t pid = ::fork();
    if (pid < 0) {
        const int32_t err = clux::fail();
        (void)::close(ends[0]);
        (void)::close(ends[1]);
        return err;
    }

    if (pid == 0) {
        (void)::close(ends[1]);
        stand_guard(ends[0]);
    }

    (void)::close(ends[0]);
    return 0;
}

CLUX_API void clux_proc_settle(void)
{
    if (ledger.fd >= 0) {
        (void)::ftruncate(ledger.fd, 0);
        ledger.fd = -1;
        ledger.used = 0;
        ledger.text[0] = '\0';
    }
}

CLUX_API int32_t clux_proc_shield(void)
{
    return clux::node::put(OOM_NODE, OOM_SHIELD);
}

CLUX_API int32_t clux_proc_limit(void)
{
    static constexpr Limit LIMITS[] = {
        {.resource = RLIMIT_CORE, .cap = 0},
        {.resource = RLIMIT_NOFILE, .cap = LIMIT_NOFILE},
        {.resource = RLIMIT_STACK, .cap = LIMIT_STACK},
        {.resource = RLIMIT_RTTIME, .cap = LIMIT_RTTIME_US},
    };
    int32_t first = 0;

    for (const Limit &limit : LIMITS) {
        const rlimit value{.rlim_cur = limit.cap, .rlim_max = limit.cap};

        if (::setrlimit(limit.resource, &value) != 0 && first == 0) {
            first = clux::fail();
        }
    }

    return first;
}

CLUX_API int32_t clux_proc_cores(void)
{
    const long count = ::sysconf(_SC_NPROCESSORS_CONF);
    return count < 1 ? -EINVAL : static_cast<int32_t>(count);
}

CLUX_API int32_t clux_proc_pin(uint64_t mask)
{
    cpu_set_t set;
    CPU_ZERO(&set);

    for (uint64_t cpu = 0; cpu < CPU_MASK_BITS; ++cpu) {
        if (((mask >> cpu) & 1U) != 0) {
            CPU_SET(static_cast<int>(cpu), &set);
        }
    }

    return ::sched_setaffinity(0, sizeof(set), &set) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_proc_realtime(uint32_t priority)
{
    sched_param param{};
    param.sched_priority = static_cast<int>(priority);
    const int policy = SCHED_FIFO | SCHED_RESET_ON_FORK;
    return ::sched_setscheduler(0, policy, &param) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_proc_clamp(uint32_t util_max)
{
    Attr attr{};
    attr.size = sizeof(Attr);
    attr.flags = ATTR_KEEP_POLICY | ATTR_KEEP_PARAMS | ATTR_CLAMP_MIN | ATTR_CLAMP_MAX;
    attr.util_min = 0;
    attr.util_max = util_max;
    return ::syscall(SYS_sched_setattr, 0L, &attr, 0UL) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_proc_ioprio(void)
{
    const long value = IOPRIO_CLASS_BEST_EFFORT << IOPRIO_CLASS_SHIFT;
    return ::syscall(SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0L, value) == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_proc_slack(uint64_t nanos)
{
    const int done = ::prctl(PR_SET_TIMERSLACK, static_cast<unsigned long>(nanos), 0UL, 0UL, 0UL);
    return done == 0 ? 0 : clux::fail();
}

CLUX_API int32_t clux_proc_lockmem(void)
{
    if (::mlockall(MCL_CURRENT | MCL_FUTURE | MCL_ONFAULT) == 0) {
        return 0;
    }

    if (errno != EINVAL) {
        return clux::fail();
    }

    return ::mlockall(MCL_CURRENT | MCL_FUTURE) == 0 ? 0 : clux::fail();
}

CLUX_API __attribute__((noreturn)) void clux_proc_abort(void)
{
    ::abort();
}
