// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#include "proc.hpp"

#include "node.hpp"

#include <fcntl.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>

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
constexpr mode_t WRITABLE_BY_OTHERS = 022;
constexpr int STD_FDS = 3;
constexpr size_t PID_CAP = 24;
constexpr const char *OOM_NODE = "/proc/self/oom_score_adj";
constexpr const char *OOM_SHIELD = "-1000\n";
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

bool trusted(const struct stat &st) noexcept
{
    return S_ISREG(st.st_mode) && st.st_uid == 0 && st.st_nlink == 1 &&
           (st.st_mode & WRITABLE_BY_OTHERS) == 0;
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

    if (!trusted(st)) {
        return -EPERM;
    }

    if (::flock(fd.get(), LOCK_EX | LOCK_NB) != 0) {
        return clux::fail();
    }

    if (::ftruncate(fd.get(), 0) != 0) {
        return clux::fail();
    }

    char text[PID_CAP];
    const int len = ::snprintf(text, sizeof(text), "%d\n", static_cast<int>(::getpid()));
    if (len <= 0 || ::pwrite(fd.get(), text, static_cast<size_t>(len), 0) != len) {
        return clux::fail();
    }

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
