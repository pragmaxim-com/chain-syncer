use std::{time::Instant};
use crate::info;

use std::sync::Mutex;

pub type BatchWeight = usize;
pub type BoxWeight = usize;

pub struct ProgressMonitor {
    min_weight_report: usize,
    start_time: Instant,
    total_and_last_report_weight: Mutex<(usize, usize)>,
}

impl ProgressMonitor {
    pub fn new(min_tx_count_report: usize) -> Self {
        ProgressMonitor {
            min_weight_report: min_tx_count_report,
            start_time: Instant::now(),
            total_and_last_report_weight: Mutex::new((0, 0)),
        }
    }

    pub fn log(
        &self,
        height: u32,
        timestamp: u32,
        batch_size: usize,
        batch_weight: &BatchWeight,
    ) {
        let mut total_weight = self.total_and_last_report_weight.lock().unwrap();
        let new_total_weight = total_weight.0 + batch_weight;
        if new_total_weight > total_weight.1 + self.min_weight_report {
            *total_weight = (new_total_weight, new_total_weight);
            let total_time = self.start_time.elapsed().as_secs();
            let txs_per_sec = format!("{:.1}", new_total_weight as f64 / total_time as f64);
            info!("{} Blocks @ {} from {} at {} ins+outs+assets per second, total {}", batch_size, height, timestamp, txs_per_sec, new_total_weight);
        } else {
            *total_weight = (new_total_weight, total_weight.1);
        }
    }
}
