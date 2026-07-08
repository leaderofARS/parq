//! `parq-metrics` — Pipeline run statistics.
//!
//! Returned by `parq_core::run_pipeline` and printed as an ASCII table.

use std::fmt;

/// Statistics gathered during one complete pipeline execution.
#[derive(Debug, Clone, Default)]
pub struct ProcessingMetrics {
    pub input_bytes:       usize,
    pub output_bytes:      usize,
    pub rows_processed:    usize,
    pub rows_errored:      usize,
    pub threads_used:      usize,
    pub total_duration_ms: u64,
    pub parse_duration_ms: u64,
    pub write_duration_ms: u64,
}

impl ProcessingMetrics {
    /// Input throughput in MiB/s.
    pub fn throughput_mib_per_sec(&self) -> f64 {
        if self.total_duration_ms == 0 { return 0.0; }
        (self.input_bytes as f64 / 1_048_576.0) / (self.total_duration_ms as f64 / 1000.0)
    }

    /// Rows per second.
    pub fn rows_per_sec(&self) -> f64 {
        if self.total_duration_ms == 0 { return 0.0; }
        self.rows_processed as f64 / (self.total_duration_ms as f64 / 1000.0)
    }

    /// Input / output size ratio.
    pub fn compression_ratio(&self) -> f64 {
        if self.output_bytes == 0 { return 0.0; }
        self.input_bytes as f64 / self.output_bytes as f64
    }

    pub fn print_report(&self) {
        println!("\n{self}");
    }
}

impl fmt::Display for ProcessingMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sep = "─".repeat(50);
        writeln!(f, "┌{sep}┐")?;
        writeln!(f, "│{:^50}│", " ⚡  Pipeline Metrics  ⚡ ")?;
        writeln!(f, "├{sep}┤")?;
        let rows: &[(&str, String)] = &[
            ("Input size",     format!("{:.2} MiB", self.input_bytes  as f64 / 1_048_576.0)),
            ("Output size",    format!("{:.2} MiB", self.output_bytes as f64 / 1_048_576.0)),
            ("Compression",    format!("{:.2}×",    self.compression_ratio())),
            ("Rows processed", format!("{}",        self.rows_processed)),
            ("Rows errored",   format!("{}",        self.rows_errored)),
            ("Threads used",   format!("{}",        self.threads_used)),
            ("Total time",     format!("{:.2?}",    std::time::Duration::from_millis(self.total_duration_ms))),
            ("  ↳ Parse",      format!("{:.2?}",    std::time::Duration::from_millis(self.parse_duration_ms))),
            ("  ↳ Write",      format!("{:.2?}",    std::time::Duration::from_millis(self.write_duration_ms))),
            ("Throughput",     format!("{:.1} MiB/s",  self.throughput_mib_per_sec())),
            ("Row rate",       format!("{:.0} rows/s", self.rows_per_sec())),
        ];
        for (label, value) in rows {
            writeln!(f, "│  {label:<22}{value:>26}  │")?;
        }
        writeln!(f, "└{sep}┘")
    }
}
