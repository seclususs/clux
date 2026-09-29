/**
 * @file detector.hpp
 *
 * @brief System environment and kernel capability detection.
 */

#pragma once

namespace qos::system
{

    /**
     * @brief Availability status of required kernel features.
     */
    struct KernelFeatures
    {
        bool has_cpu_psi{false};       ///< Indicates CPU Pressure Stall Information support.
        bool has_io_psi{false};        ///< Indicates I/O Pressure Stall Information support.
    };

    /**
     * @brief Utility for probing system capabilities.
     */
    class Detector
    {
        public:
        /**
         * @brief Scans the environment to determine kernel feature availability.
         *
         * @return A structure representing detected capabilities.
         */
        [[nodiscard]] static KernelFeatures check_features() noexcept;
    };

} // namespace qos::system