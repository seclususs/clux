//! Author: [Seclususs](https://github.com/seclususs)

use crate::algorithms::{cpu, helpers, poller, thermal};
use crate::config::{limits, paths, runtime};
use crate::daemon::{state, traits, types};
use crate::hal::{kernel, monitors, sensors, sysfs};

use std::{fs, io, os, time};

#[derive(Debug, Clone, Copy)]
struct ControllerConfig {
    poll_weight_pressure: f32,
    poll_weight_derivative: f32,
    battery_check_interval_sec: u64,
    psi_threshold_us: i32,
    psi_window_us: i32,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            poll_weight_pressure: 1.5,
            poll_weight_derivative: 0.05,
            battery_check_interval_sec: 5,
            psi_threshold_us: 100_000,
            psi_window_us: 1_000_000,
        }
    }
}

pub struct CpuController {
    trigger_fd: fs::File,
    latency: sysfs::CachedFile,
    min_granularity: sysfs::CachedFile,
    wakeup: sysfs::CachedFile,
    migration: sysfs::CachedFile,
    walt_init: sysfs::CachedFile,
    uclamp_min: sysfs::CachedFile,
    psi_cpu_monitor: monitors::PsiMonitor,
    thermal_manager: thermal::ThermalManager,
    thermal_config: thermal::ThermalConfig,
    cpu_sensor: sensors::ThermalSensor,
    battery_sensor: sensors::ThermalSensor,
    battery_capacity_sensor: sensors::BatterySensor,
    cached_battery_level: f32,
    cached_battery_temp: f32,
    last_battery_check: time::Instant,
    current_latency: f32,
    current_min_granularity: f32,
    current_wakeup: f32,
    current_migration: f32,
    current_walt_init: f32,
    current_uclamp_min: f32,
    load_state: cpu::LoadState,
    cpu_math_config: cpu::CpuMathConfig,
    cpu_kernel_limits: cpu::CpuLimits,
    controller_config: ControllerConfig,
    last_tick: time::Instant,
    adaptive_poller: poller::AdaptivePoller,
    next_wake_ms: i32,
}

impl CpuController {
    pub fn new() -> types::Result<Self> {
        log::debug!("Initializing CPU Controller...");

        let config_limits = limits::CpuLimitsConfig::default();

        let cpu_math_config = cpu::CpuMathConfig::default();
        let controller_config = ControllerConfig::default();

        let cpu_kernel_limits = cpu::CpuLimits {
            min_latency_ns: config_limits.min_latency_ns as f32,
            max_latency_ns: config_limits.max_latency_ns as f32,
            min_granularity_ns: config_limits.min_granularity_ns as f32,
            max_granularity_ns: config_limits.max_granularity_ns as f32,
            min_wakeup_ns: config_limits.min_wakeup_ns as f32,
            max_wakeup_ns: config_limits.max_wakeup_ns as f32,
            min_migration_cost: config_limits.min_migration_cost as f32,
            max_migration_cost: config_limits.max_migration_cost as f32,
            min_walt_init_pct: config_limits.min_walt_init_pct as f32,
            max_walt_init_pct: config_limits.max_walt_init_pct as f32,
            min_uclamp_min: config_limits.min_uclamp_min as f32,
            max_uclamp_min: config_limits.max_uclamp_min as f32,
        };

        let raw_trigger_fd = kernel::register_psi_trigger(
            paths::K_PSI_CPU_PATH,
            controller_config.psi_threshold_us,
            controller_config.psi_window_us,
        )
        .map_err(|e| types::QosError::FfiError(format!("CPU PSI Trigger Error: {e}").into()))?;
        let trigger_fd = unsafe { os::fd::FromRawFd::from_raw_fd(raw_trigger_fd) };

        let latency = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_LATENCY_NS).ok(),
            0,
        );

        let min_granularity = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_MIN_GRANULARITY_NS).ok(),
            0,
        );

        let wakeup = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_WAKEUP_GRANULARITY_NS).ok(),
            0,
        );

        let migration = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_MIGRATION_COST_NS).ok(),
            0,
        );

        let walt_init = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_WALT_INIT_TASK_LOAD_PCT).ok(),
            config_limits.min_walt_init_pct,
        );

        let uclamp_min = sysfs::CachedFile::new_opt(
            sysfs::open_file_for_write(paths::K_SCHED_UCLAMP_UTIL_MIN).ok(),
            config_limits.min_uclamp_min,
        );

        let psi_cpu_monitor = monitors::PsiMonitor::new(paths::K_PSI_CPU_PATH)?;

        let cpu_path = paths::get_cpu_temp_path();
        let cpu_sensor = sensors::ThermalSensor::new(cpu_path.to_str().unwrap_or_default(), 70.0);

        let battery_sensor = sensors::ThermalSensor::new(paths::K_BATTERY_TEMP_PATH, 35.0);
        let battery_capacity_sensor = sensors::BatterySensor::new(paths::K_BATTERY_CAPACITY_PATH);

        let thermal_config = thermal::ThermalConfig::default();
        let thermal_manager = thermal::ThermalManager::default();

        let adaptive_poller = poller::AdaptivePoller::new(
            controller_config.poll_weight_pressure,
            controller_config.poll_weight_derivative,
            poller::PollerConfig::default(),
        );

        let mut controller = Self {
            trigger_fd,
            latency,
            min_granularity,
            wakeup,
            migration,
            walt_init,
            uclamp_min,
            psi_cpu_monitor,
            thermal_manager,
            thermal_config,
            cpu_sensor,
            battery_sensor,
            battery_capacity_sensor,
            cached_battery_level: 50.0,
            cached_battery_temp: 35.0,
            last_battery_check: time::Instant::now(),
            current_latency: config_limits.min_latency_ns as f32,
            current_min_granularity: config_limits.min_granularity_ns as f32,
            current_wakeup: config_limits.min_wakeup_ns as f32,
            current_migration: config_limits.min_migration_cost as f32,
            current_walt_init: config_limits.min_walt_init_pct as f32,
            current_uclamp_min: config_limits.min_uclamp_min as f32,
            load_state: cpu::LoadState::default(),
            cpu_math_config,
            cpu_kernel_limits,
            controller_config,
            last_tick: time::Instant::now(),
            adaptive_poller,
            next_wake_ms: runtime::MIN_POLLING_MS as i32,
        };

        controller.cached_battery_level = controller.battery_capacity_sensor.read();
        controller.cached_battery_temp = controller.battery_sensor.read();
        controller.apply_values(true);

        Ok(controller)
    }

    fn update_cpu_logic(&mut self, context: &mut state::DaemonContext) -> types::Result<()> {
        let cpu_psi_data = self.psi_cpu_monitor.read_state()?;
        let cpu_some_trend = cpu_psi_data.some;
        let current_io_psi = context.pressure.io_psi;
        let target_psi = cpu_some_trend.current;
        let is_structural_break = cpu_some_trend.nis > self.cpu_math_config.nis_threshold;
        let cpu_temp = self.cpu_sensor.read();
        let now = time::Instant::now();

        if now.duration_since(self.last_battery_check).as_secs()
            >= self.controller_config.battery_check_interval_sec
        {
            self.cached_battery_level = self.battery_capacity_sensor.read();
            self.cached_battery_temp = self.battery_sensor.read();
            self.last_battery_check = now;
        }

        let battery_level = self.cached_battery_level;
        let battery_temp = self.cached_battery_temp;

        let thermal_scale = self.thermal_manager.update(
            cpu_temp,
            battery_temp,
            target_psi,
            &self.thermal_config,
            now,
        );

        let trend_factor = cpu::calculate_trend_gain(cpu_some_trend.velocity);

        let elapsed_duration = now.duration_since(self.last_tick);
        self.last_tick = now;
        let elapsed_sec = elapsed_duration.as_secs_f32().max(0.000_001);
        let safe_elapsed_sec = cpu::sanitize_dt(elapsed_sec);

        let (integral_total, integral_dot) = cpu::update_integral_params(
            &mut self.load_state,
            battery_level,
            safe_elapsed_sec,
            &self.cpu_math_config,
        );

        let demand_input = cpu::DemandInput {
            target_psi,
            psi_velocity: cpu_some_trend.velocity,
            dt_real: elapsed_sec,
            dt_safe: safe_elapsed_sec,
            thermal_scale,
            trend_factor,
            integral_total,
            integral_dot,
            is_structural_break,
        };

        let load_demand =
            cpu::calculate_load_demand(&mut self.load_state, demand_input, &self.cpu_math_config);

        let effective_pressure = cpu::calculate_effective_pressure(
            load_demand,
            trend_factor,
            current_io_psi,
            &self.cpu_math_config,
        );

        context.pressure.cpu_psi = effective_pressure;

        let mut calculated_poll_interval_ms = self.adaptive_poller.calculate_next_interval(
            effective_pressure,
            cpu_some_trend.avg300,
            cpu_some_trend.velocity,
        ) as i32;

        if cpu::is_transient(&self.load_state, target_psi, &self.cpu_math_config) {
            calculated_poll_interval_ms = calculated_poll_interval_ms
                .min(self.cpu_math_config.transient_poll_interval as i32);
        }

        self.next_wake_ms = calculated_poll_interval_ms;

        let thermal_min_latency_ns =
            cpu::calculate_thermal_latency_limit(thermal_scale, &self.cpu_kernel_limits);

        let (target_latency, target_min_granularity) = cpu::calculate_latency_and_granularity(
            effective_pressure,
            load_demand,
            thermal_min_latency_ns,
            &self.cpu_math_config,
            &self.cpu_kernel_limits,
        );

        let target_migration = cpu::calculate_migration_cost(
            cpu_some_trend.velocity,
            effective_pressure,
            &self.cpu_kernel_limits,
        );

        let target_wakeup = cpu::calculate_wakeup_granularity(
            effective_pressure,
            &self.cpu_math_config,
            &self.cpu_kernel_limits,
        );

        let target_walt_init =
            cpu::calculate_walt_init(effective_pressure, &self.cpu_kernel_limits);

        let target_uclamp = cpu::calculate_uclamp_min(
            effective_pressure,
            thermal_scale,
            &self.cpu_math_config,
            &self.cpu_kernel_limits,
        );

        self.current_latency = target_latency;
        self.current_min_granularity = target_min_granularity;
        self.current_wakeup = target_wakeup;
        self.current_migration = target_migration;
        self.current_walt_init = target_walt_init;
        self.current_uclamp_min = target_uclamp;
        self.apply_values(false);
        Ok(())
    }

    fn apply_values(&mut self, force: bool) {
        let latency_u64 = helpers::sanitize_to_clean_u64(
            self.current_latency,
            self.cpu_kernel_limits.max_latency_ns as u64,
            50_000,
        );

        let granularity_u64 = helpers::sanitize_to_clean_u64(
            self.current_min_granularity,
            self.cpu_kernel_limits.max_granularity_ns as u64,
            50_000,
        );

        let wakeup_u64 = helpers::sanitize_to_clean_u64(
            self.current_wakeup,
            self.cpu_kernel_limits.max_wakeup_ns as u64,
            50_000,
        );

        let migration_u64 = helpers::sanitize_to_clean_u64(
            self.current_migration,
            self.cpu_kernel_limits.min_migration_cost as u64,
            50_000,
        );

        let walt_u64 = helpers::sanitize_to_u64(
            self.current_walt_init,
            self.cpu_kernel_limits.min_walt_init_pct as u64,
        );

        let uclamp_u64 = helpers::sanitize_to_u64(
            self.current_uclamp_min,
            self.cpu_kernel_limits.min_uclamp_min as u64,
        );

        self.latency
            .update(latency_u64, force, &sysfs::CheckStrategy::Relative(100));

        self.min_granularity
            .update(granularity_u64, force, &sysfs::CheckStrategy::Relative(100));

        self.wakeup
            .update(wakeup_u64, force, &sysfs::CheckStrategy::Relative(150));

        self.migration
            .update(migration_u64, force, &sysfs::CheckStrategy::Absolute(50000));

        self.walt_init
            .update(walt_u64, force, &sysfs::CheckStrategy::Absolute(5));

        self.uclamp_min
            .update(uclamp_u64, force, &sysfs::CheckStrategy::Absolute(32));
    }
}

impl traits::EventHandler for CpuController {
    fn as_raw_fd(&self) -> Option<os::fd::RawFd> {
        Some(os::fd::AsRawFd::as_raw_fd(&self.trigger_fd))
    }

    fn on_event(
        &mut self,
        context: &mut state::DaemonContext,
    ) -> types::Result<traits::LoopAction> {
        let mut buffer = [0u8; 8];
        let _ = io::Read::read(&mut self.trigger_fd, &mut buffer);
        self.update_cpu_logic(context)?;
        Ok(traits::LoopAction::Continue)
    }

    fn on_timeout(
        &mut self,
        context: &mut state::DaemonContext,
    ) -> types::Result<traits::LoopAction> {
        self.update_cpu_logic(context)?;
        Ok(traits::LoopAction::Continue)
    }

    fn get_timeout_ms(&self) -> i32 {
        self.next_wake_ms
    }

    fn get_poll_flags(&self) -> rustix::event::epoll::EventFlags {
        rustix::event::epoll::EventFlags::PRI | rustix::event::epoll::EventFlags::ERR
    }
}
