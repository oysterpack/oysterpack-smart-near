use crate::AccountMetrics;

/// Tracks account events and collects stats for reporting purposes
pub trait GetAccountMetrics {
    fn account_metrics(&self) -> AccountMetrics {
        AccountMetrics::load()
    }
}
