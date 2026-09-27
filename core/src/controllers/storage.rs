//! Author: [Seclususs](https://github.com/seclususs)

use crate::algorithms::{helpers, poller, storage};
use crate::config::{limits, paths, runtime};
use crate::daemon::{state, traits, types};
use crate::hal::{kernel, monitors, sysfs};

use std::{fs, io, os, time};

#[derive(Debug, Clone, Copy)]
struct ControllerConfig {
    poll_weight_pressure: f32,
    poll_weight_derivative: f32,
    psi_threshold_us: i32,
    psi_window_us: i32,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            poll_weight_pressure: 1.2,
            poll_weight_derivative: 0.08,
            psi_threshold_us: 250_000,
            psi_window_us: 1_000_000,
        }
    }
}

pub struct StorageController {
    trigger_fd: fs::File,
    read_ahead: sysfs::CachedFile,
    nr_requests: sysfs::CachedFile,
    psi_io_monitor: monitors::PsiMonitor,
    disk_monitor: monitors::DiskMonitor,
    prev_io_stats: monitors::IoStats,
    workload_state: storage::WorkloadState,
    storage_math_config: storage::StorageMathConfig,
    storage_kernel_limits: storage::StorageLimits,
    last_tick: time::Instant,
    current_read_ahead: f32,
    current_nr_requests: f32,
    adaptive_poller: poller::AdaptivePoller,
    next_wake_ms: i32,
}

impl StorageController {
    pub fn new() -> types::Result<Self> {
        log::debug!("Initializing Storage Controller...");

        let config_limits = limits::StorageLimitsConfig::default();

        let storage_math_config = storage::StorageMathConfig::default();
        let controller_config = ControllerConfig::default();

        let storage_kernel_limits = storage::StorageLimits {
            min_read_ahead: config_limits.min_read_ahead as f32,
            max_read_ahead: config_limits.max_read_ahead as f32,
            min_nr_requests: config_limits.min_nr_requests as f32,
            max_nr_requests: config_limits.max_nr_requests as f32,
        };

        let raw_trigger_fd = kernel::register_psi_trigger(
            paths::K_PSI_IO_PATH,
            controller_config.psi_threshold_us,
            controller_config.psi_window_us,
        )
        .map_err(|e| {
            types::QosError::FfiError(format!("Storage I/O PSI trigger Error: {e}").into())
        })?;
        let trigger_fd = unsafe { os::fd::FromRawFd::from_raw_fd(raw_trigger_fd) };

        let read_ahead_path = paths::get_read_ahead_path();
        let nr_requests_path = paths::get_nr_requests_path();

        let read_ahead = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(read_ahead_path.to_str().unwrap_or_default()).ok(),
            0,
        );

        let nr_requests = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(nr_requests_path.to_str().unwrap_or_default()).ok(),
            0,
        );

        if !read_ahead.is_active() && !nr_requests.is_active() {
            return Err(types::QosError::SystemCheckFailed(
                "No storage block tunables found.".into(),
            ));
        }

        let psi_io_monitor = monitors::PsiMonitor::new(paths::K_PSI_IO_PATH)?;

        let stats_path = paths::get_diskstats_path();
        let mut disk_monitor = monitors::DiskMonitor::new(stats_path.to_str().unwrap_or_default())?;

        let initial_stats = disk_monitor
            .read_stats()
            .unwrap_or(monitors::IoStats::default());

        let adaptive_poller = poller::AdaptivePoller::new(
            controller_config.poll_weight_pressure,
            controller_config.poll_weight_derivative,
            poller::PollerConfig::default(),
        );

        let mut controller = Self {
            trigger_fd,
            read_ahead,
            nr_requests,
            psi_io_monitor,
            disk_monitor,
            prev_io_stats: initial_stats,
            workload_state: storage::WorkloadState::default(),
            storage_math_config,
            storage_kernel_limits,
            last_tick: time::Instant::now(),
            current_read_ahead: config_limits.min_read_ahead as f32,
            current_nr_requests: config_limits.max_nr_requests as f32,
            adaptive_poller,
            next_wake_ms: runtime::MIN_POLLING_MS as i32,
        };

        controller.apply_values(true);
        Ok(controller)
    }

    fn update_io_logic(&mut self, context: &mut state::DaemonContext) -> types::Result<()> {
        let psi_data = self.psi_io_monitor.read_state()?;
        let current_io_stats = self.disk_monitor.read_stats()?;
        let now = time::Instant::now();
        let elapsed_duration = now.duration_since(self.last_tick);
        self.last_tick = now;
        let elapsed_sec = elapsed_duration.as_secs_f32().max(0.000_001);
        let stats_delta =
            storage::calculate_io_deltas(&current_io_stats, &self.prev_io_stats, elapsed_sec);
        self.prev_io_stats = current_io_stats;

        context.pressure.io_psi = psi_data.some.current;
        context.pressure.io_saturation = current_io_stats.in_flight as f32;

        if current_io_stats.in_flight == 0 && psi_data.some.current < 0.10 {
            self.next_wake_ms = self.storage_math_config.idle_poll_interval.max(500.0) as i32;
            return Ok(());
        }

        let request_size_ratio =
            storage::calculate_request_size_ratio(&stats_delta, &self.storage_math_config);
        let merge_ratio = storage::calculate_merge_ratio(&stats_delta);
        let pressure_ratio = storage::calculate_pressure_ratio(
            current_io_stats.in_flight as f32,
            &self.storage_math_config,
        );

        let sequentiality = storage::resolve_sequentiality_factor(
            &mut self.workload_state,
            request_size_ratio,
            merge_ratio,
            pressure_ratio,
            &self.storage_math_config,
        );

        let calculated_read_ahead =
            storage::calculate_target_read_ahead(sequentiality, &self.storage_kernel_limits);

        let effective_throughput =
            storage::calculate_weighted_throughput(&stats_delta, &self.storage_math_config);

        let target_latency =
            storage::calculate_target_latency(psi_data.some.current, &self.storage_math_config);

        let current_latency = storage::calculate_effective_latency(
            &stats_delta,
            effective_throughput,
            current_io_stats.in_flight as f32,
        );

        let calculated_nr_requests = storage::calculate_next_queue_depth(
            effective_throughput,
            current_latency,
            target_latency,
            self.current_nr_requests,
            psi_data.some.current,
            &self.storage_math_config,
            &self.storage_kernel_limits,
        );

        if storage::should_update_nr_requests(
            calculated_nr_requests,
            self.current_nr_requests,
            &self.storage_math_config,
            &self.storage_kernel_limits,
        ) {
            self.current_nr_requests = calculated_nr_requests;
        }
        self.current_read_ahead = calculated_read_ahead;

        if storage::is_congestion_critical(
            psi_data.some.current,
            current_io_stats.in_flight as f32,
            &self.storage_math_config,
        ) {
            self.next_wake_ms = runtime::MIN_POLLING_MS as i32;
        } else {
            self.next_wake_ms = self.adaptive_poller.calculate_next_interval(
                psi_data.some.current,
                psi_data.some.avg300,
                psi_data.some.velocity,
            ) as i32;
        }

        self.apply_values(false);
        Ok(())
    }

    fn apply_values(&mut self, force: bool) {
        let read_ahead_u64 = helpers::sanitize_to_clean_u64(
            self.current_read_ahead,
            self.storage_kernel_limits.max_read_ahead as u64,
            32,
        );

        let nr_requests_u64 = helpers::sanitize_to_clean_u64(
            self.current_nr_requests,
            self.storage_kernel_limits.min_nr_requests as u64,
            16,
        );

        self.read_ahead
            .update(read_ahead_u64, force, &sysfs::CheckStrategy::Absolute(32));

        self.nr_requests
            .update(nr_requests_u64, force, &sysfs::CheckStrategy::Absolute(16));
    }
}

impl traits::EventHandler for StorageController {
    fn as_raw_fd(&self) -> Option<os::fd::RawFd> {
        Some(os::fd::AsRawFd::as_raw_fd(&self.trigger_fd))
    }

    fn on_event(
        &mut self,
        context: &mut state::DaemonContext,
    ) -> types::Result<traits::LoopAction> {
        let mut buffer = [0u8; 8];
        let _ = io::Read::read(&mut self.trigger_fd, &mut buffer);
        self.update_io_logic(context)?;
        Ok(traits::LoopAction::Continue)
    }

    fn on_timeout(
        &mut self,
        context: &mut state::DaemonContext,
    ) -> types::Result<traits::LoopAction> {
        self.update_io_logic(context)?;
        Ok(traits::LoopAction::Continue)
    }

    fn get_timeout_ms(&self) -> i32 {
        self.next_wake_ms
    }

    fn get_poll_flags(&self) -> rustix::event::epoll::EventFlags {
        rustix::event::epoll::EventFlags::PRI | rustix::event::epoll::EventFlags::ERR
    }
}
