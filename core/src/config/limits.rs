//! Author: [Seclususs](https://github.com/seclususs)

#[derive(Debug, Clone, Copy)]
pub struct CpuLimitsConfig {
    pub min_latency_ns: u64,
    pub max_latency_ns: u64,
    pub min_granularity_ns: u64,
    pub max_granularity_ns: u64,
    pub min_wakeup_ns: u64,
    pub max_wakeup_ns: u64,
    pub min_migration_cost: u64,
    pub max_migration_cost: u64,
    pub min_walt_init_pct: u64,
    pub max_walt_init_pct: u64,
    pub min_uclamp_min: u64,
    pub max_uclamp_min: u64,
}

impl Default for CpuLimitsConfig {
    fn default() -> Self {
        Self {
            min_latency_ns: 8_000_000,
            max_latency_ns: 20_000_000,
            min_granularity_ns: 2_500_000,
            max_granularity_ns: 6_500_000,
            min_wakeup_ns: 1_500_000,
            max_wakeup_ns: 6_500_000,
            min_migration_cost: 200_000,
            max_migration_cost: 600_000,
            min_walt_init_pct: 10,
            max_walt_init_pct: 40,
            min_uclamp_min: 0,
            max_uclamp_min: 384,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StorageLimitsConfig {
    pub min_read_ahead: u64,
    pub max_read_ahead: u64,
    pub min_nr_requests: u64,
    pub max_nr_requests: u64,
}

impl Default for StorageLimitsConfig {
    fn default() -> Self {
        Self {
            min_read_ahead: 128,
            max_read_ahead: 1024,
            min_nr_requests: 64,
            max_nr_requests: 256,
        }
    }
}

