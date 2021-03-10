use crate::AccountStats;

/// Tracks account events and collects stats for reporting purposes
pub trait AccountReporting {
    fn account_stats(&self) -> AccountStats {
        AccountStats::load()
    }
}
