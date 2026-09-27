#include "daemon/daemon.hpp"
#include "daemon/crash.hpp"
#include "daemon/detector.hpp"
#include "daemon/logger.hpp"
#include "daemon/tuner.hpp"
#include "qos/bridge.hpp"

#include <csignal>
#include <cstdlib>
#include <fcntl.h>
#include <malloc.h>
#include <sys/file.h>
#include <sys/signalfd.h>
#include <unistd.h>

#ifndef M_DECAY_TIME
#define M_DECAY_TIME -100
#endif

#ifndef M_PURGE
#define M_PURGE -101
#endif

namespace qos::core
{

    int App::bootstrap() noexcept
    {
        if (::getuid() != 0)
        {
            LOGE("Shutting down (Root privileges required).");
            return EXIT_FAILURE;
        }

        int lock_fd = ::open("/data/local/tmp/qos_daemon.lock", O_CREAT | O_RDWR | O_CLOEXEC, 0666);

        if (lock_fd < 0 || ::flock(lock_fd, LOCK_EX | LOCK_NB) == -1)
        {
            if (lock_fd >= 0)
            {
                ::close(lock_fd);
            }

            return EXIT_FAILURE;
        }

        ::mallopt(M_DECAY_TIME, 0);
        LOGI("=== Daemon Starting ===");

        LOGD("Hardening Environment...");
        qos::core::Handler::arm();
        qos::system::Tuner::harden_process();
        qos::system::Tuner::expand_resources();
        qos::system::Tuner::enforce_efficiency_mode();
        qos::system::Tuner::set_realtime_policy();
        qos::system::Tuner::maximize_timer_slack();
        qos::system::Tuner::limit_cpu_utilization();
        qos::system::Tuner::set_high_io_priority();

        LOGD("Checking Hardware Support...");
        const auto features = qos::system::Detector::check_features();
        (void)features; // Avoid unused variable warning

        LOGD("Activating Services...");

        ::mallopt(M_PURGE, 0);
        qos::system::Tuner::lock_memory();

        sigset_t mask;
        ::sigemptyset(&mask);
        ::sigaddset(&mask, SIGINT);
        ::sigaddset(&mask, SIGTERM);
        ::sigaddset(&mask, SIGHUP);
        ::sigprocmask(SIG_BLOCK, &mask, nullptr);

        const int sfd = ::signalfd(-1, &mask, SFD_CLOEXEC | SFD_NONBLOCK);

        LOGI("Handover to Core Logic...");
        const int rust_status = ::start_services(sfd);

        if (rust_status != 0)
        {
            LOGE("Core services failed to start (Error: %d).", rust_status);
            return EXIT_FAILURE;
        }

        LOGD("Core services running. Main thread waiting...");
        ::join_threads();

        LOGI("=== Shutdown Cleanly ===");
        return EXIT_SUCCESS;
    }

} // namespace qos::core