use crate::AccountStats;

/// Tracks account events and collects stats
pub trait AccountTracking {
    fn account_stats(&self) -> AccountStats;
}
