//! `parq-core` — Pipeline orchestrator.
//!
//! Ties together all parq sub-crates into a single `run_pipeline` call:
//!
//! ```text
//! parq-chunk   →  fixed-size 64 MiB windows
//! parq-schema  →  Arrow schema (inferred or explicit)
//! parq-parser  →  parallel JSON → RecordBatch (rayon)
//! crossbeam    →  bounded channel (backpressure)
//! parq-io      →  streaming Parquet writer (dedicated thread)
//! parq-metrics →  timing + throughput statistics
//! ```
//!
//! ## Memory safety profile
//!
//! The only `unsafe` block in the entire workspace is the `mmap` call below.
//! The safety invariant is:
//!
//! > *We hold an exclusive read-only `File` handle.  No external process
//! > writes to this file for the duration of `run_pipeline`.*
//!
//! After the mmap, Rust's lifetime system takes over: `parq_chunk::fixed_chunks`
//! returns `Vec<&'a [u8]>` whose lifetime `'a` is tied to the `Mmap` on the
//! stack — the compiler statically prevents use-after-free.

pub use parq_error::ParqError;
pub use parq_metrics::ProcessingMetrics;

use anyhow::Result;
use crossbeam_channel::bounded;
use memmap2::MmapOptions;
use parquet::basic::Compression;
use rayon::prelude::*;
use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};
use tracing::{info, warn};

/// Full pipeline configuration.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub input_path:    String,
    pub output_path:   String,
    pub batch_size:    usize,
    pub compression:   Compression,
    pub num_threads:   usize,
    /// Lenient: skip malformed records, log to stderr.
    pub ignore_errors: bool,
    /// Flatten nested JSON objects (`user.id` → `user_id`).
    pub flatten:       bool,
    pub limit:         Option<usize>,
    /// JSON schema file path (skips auto-inference when set).
    pub schema_path:   Option<String>,
    /// Bounded channel depth — number of `RecordBatch`es in flight.
    pub channel_depth: usize,
    /// Raw chunk size override (bytes). `None` = 64 MiB default.
    pub chunk_size:    Option<usize>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            input_path:    String::new(),
            output_path:   String::new(),
            batch_size:    65_536,
            compression:   Compression::SNAPPY,
            num_threads:   0,
            ignore_errors: false,
            flatten:       false,
            limit:         None,
            schema_path:   None,
            channel_depth: 8,
            chunk_size:    None,
        }
    }
}

/// Execute the full NDJSON → Parquet pipeline.
///
/// # Errors
/// I/O errors, schema inference failures, strict-mode parse errors,
/// and Parquet writer errors are all propagated.
pub fn run_pipeline(config: PipelineConfig) -> Result<ProcessingMetrics> {
    let start = Instant::now();

    // ── Thread pool ───────────────────────────────────────────────────
    let threads = if config.num_threads == 0 { num_cpus::get() } else { config.num_threads };
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .unwrap_or(());
    info!(threads, "parq pipeline starting");

    // ── Memory-map ────────────────────────────────────────────────────
    let input_file = File::open(&config.input_path)?;
    let file_size  = input_file.metadata()?.len() as usize;
    info!(path = %config.input_path,
          size_mib = format!("{:.2}", file_size as f64 / 1_048_576.0),
          "Memory-mapping input");

    // SAFETY: exclusive read-only handle; no external writer during pipeline.
    let mmap      = unsafe { MmapOptions::new().map(&input_file)? };
    let raw_bytes : &[u8] = &mmap;

    // ── Schema ────────────────────────────────────────────────────────
    let schema = if let Some(ref sp) = config.schema_path {
        info!(path = %sp, "Loading explicit schema");
        parq_schema::load_schema_from_file(sp)?
    } else {
        info!("Inferring schema from first 10,000 records");
        parq_schema::infer_schema(raw_bytes, 10_000)?
    };
    let schema = Arc::new(schema);
    info!(fields = schema.fields().len(), "Schema ready");

    // ── Fixed 64 MiB chunks ───────────────────────────────────────────
    let chunks = parq_chunk::fixed_chunks(raw_bytes, config.chunk_size);
    info!(num_chunks = chunks.len(),
          chunk_mib  = config.chunk_size.unwrap_or(parq_chunk::CHUNK_SIZE_BYTES) / 1_048_576,
          "Input partitioned");

    // ── Bounded channel ───────────────────────────────────────────────
    // When the channel is full, rayon workers block on `send()` —
    // this is the backpressure that prevents RAM from ballooning.
    let (tx, rx) = bounded::<arrow::record_batch::RecordBatch>(config.channel_depth);

    // ── Dedicated writer thread ───────────────────────────────────────
    let out_path   = config.output_path.clone();
    let schema_w   = Arc::clone(&schema);
    let compression = config.compression;
    let write_start = Instant::now();

    let writer_handle = thread::spawn(move || -> Result<()> {
        let mut w = parq_io::ParquetStreamWriter::new(
            Path::new(&out_path), schema_w, compression,
        )?;
        for batch in rx { w.write_batch(&batch)?; }
        w.close()
    });

    // ── Parallel parse ────────────────────────────────────────────────
    let total_rows = Arc::new(AtomicUsize::new(0));
    let error_rows = Arc::new(AtomicUsize::new(0));
    let parse_start = Instant::now();

    chunks.par_iter().for_each_with(tx, |tx, chunk| {
        match parq_parser::parse_chunk(
            chunk, Arc::clone(&schema),
            config.batch_size, config.ignore_errors,
            config.flatten,    config.limit,
        ) {
            Ok((batches, rows, skipped)) => {
                total_rows.fetch_add(rows,    Ordering::Relaxed);
                error_rows.fetch_add(skipped, Ordering::Relaxed);
                for batch in batches {
                    if tx.send(batch).is_err() { warn!("Writer channel closed early"); }
                }
            }
            Err(e) => tracing::error!("Chunk error: {e}"),
        }
    });
    // All senders dropped → writer's `rx` loop exits cleanly.

    let parse_elapsed = parse_start.elapsed();
    let rows_parsed   = total_rows.load(Ordering::SeqCst);
    let rows_skipped  = error_rows.load(Ordering::SeqCst);
    info!(rows_parsed, rows_skipped, elapsed = ?parse_elapsed, "Parse complete");

    // ── Join writer ───────────────────────────────────────────────────
    writer_handle
        .join()
        .map_err(|_| anyhow::anyhow!("Writer thread panicked"))??;

    let write_elapsed = write_start.elapsed();
    let output_size   = std::fs::metadata(&config.output_path)
        .map(|m| m.len() as usize).unwrap_or(0);
    info!(size_mib = format!("{:.2}", output_size as f64 / 1_048_576.0),
          elapsed = ?write_elapsed, "Write complete");

    Ok(ProcessingMetrics {
        input_bytes:       file_size,
        output_bytes:      output_size,
        rows_processed:    rows_parsed,
        rows_errored:      rows_skipped,
        threads_used:      threads,
        total_duration_ms: start.elapsed().as_millis() as u64,
        parse_duration_ms: parse_elapsed.as_millis() as u64,
        write_duration_ms: write_elapsed.as_millis() as u64,
    })
}
