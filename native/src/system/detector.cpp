#include "daemon/detector.hpp"
#include "daemon/logger.hpp"

#include <sys/statvfs.h>
#include <unistd.h>

namespace qos::system
{
    namespace
    {

        [[nodiscard]] inline bool can_access(const char* path, int mode = R_OK) noexcept
        {
            return ::access(path, mode) == 0;
        }

    } // namespace

    KernelFeatures Detector::check_features() noexcept
    {
        KernelFeatures features;

        if (!can_access("/proc/pressure", R_OK | X_OK))
        {
            LOGE("/proc/pressure is missing. All PSI metrics disabled.");
            return features;
        }

        features.has_cpu_psi = can_access("/proc/pressure/cpu");

        if (features.has_cpu_psi)
        {
            LOGD("PSI CPU DETECTED.");
        }
        else
        {
            LOGW("PSI CPU MISSING.");
        }

        features.has_io_psi = can_access("/proc/pressure/io");

        if (features.has_io_psi)
        {
            LOGD("PSI I/O DETECTED.");
        }
        else
        {
            LOGW("PSI I/O MISSING.");
        }

        return features;
    }

} // namespace qos::system