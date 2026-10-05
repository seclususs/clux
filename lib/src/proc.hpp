#pragma once

#include "base.hpp"

CLUX_API int32_t clux_proc_root(void);
CLUX_API int32_t clux_proc_detach(void);
CLUX_API int32_t clux_proc_lock(const char *path);
CLUX_API int32_t clux_proc_shield(void);
CLUX_API int32_t clux_proc_limit(void);
CLUX_API int32_t clux_proc_cores(void);
CLUX_API int32_t clux_proc_pin(uint64_t mask);
CLUX_API int32_t clux_proc_realtime(uint32_t priority);
CLUX_API int32_t clux_proc_clamp(uint32_t util_max);
CLUX_API int32_t clux_proc_ioprio(void);
CLUX_API int32_t clux_proc_slack(uint64_t nanos);
CLUX_API int32_t clux_proc_lockmem(void);
CLUX_API __attribute__((noreturn)) void clux_proc_abort(void);
