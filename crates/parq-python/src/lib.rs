//! Python bindings for the `parq` zero-copy preprocessor.
//!
//! Exposes the [`convert`] function and [`ConversionMetrics`] class to Python.
//! Releases the GIL (`py.allow_threads`) during parsing to ensure true concurrency.

use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use parq_core::{run_pipeline, PipelineConfig};
use parquet::basic::Compression;

/// Detailed processing metrics returned to Python.
/// Python's GC only touches this lightweight struct (~64 bytes).
#[pyclass]
#[derive(Clone, Copy)]
pub struct ConversionMetrics {
    #[pyo3(get)]
    pub input_bytes: usize,
    #[pyo3(get)]
    pub output_bytes: usize,
    #[pyo3(get)]
    pub rows_processed: usize,
    #[pyo3(get)]
    pub rows_errored: usize,
    #[pyo3(get)]
    pub threads_used: usize,
    #[pyo3(get)]
    pub total_duration_ms: u64,
    #[pyo3(get)]
    pub parse_duration_ms: u64,
    #[pyo3(get)]
    pub write_duration_ms: u64,
}

/// Convert a newline-delimited JSON (NDJSON) file to Apache Parquet.
///
/// This function releases the Python GIL during execution, allowing Python's
/// thread scheduler to run other threads concurrently (e.g. download scripts).
/// Memory usage stays constant as data is mapped directly by the OS.
#[pyfunction]
#[pyo3(signature = (
    input_path,
    output_path,
    compression = "snappy",
    threads = 0,
    batch_size = 65536,
    channel_depth = 8,
    ignore_errors = false,
    flatten = false,
    limit = None,
    schema = None,
    chunk_size = None
))]
#[allow(clippy::too_many_arguments)]
fn convert(
    py: Python,
    input_path: String,
    output_path: String,
    compression: &str,
    threads: usize,
    batch_size: usize,
    channel_depth: usize,
    ignore_errors: bool,
    flatten: bool,
    limit: Option<usize>,
    schema: Option<String>,
    chunk_size: Option<usize>,
) -> PyResult<ConversionMetrics> {
    // 1. Parse compression option before releasing the GIL
    let comp = match compression.to_lowercase().as_str() {
        "snappy"                => Compression::SNAPPY,
        "gzip"                  => Compression::GZIP(Default::default()),
        "brotli"                => Compression::BROTLI(Default::default()),
        "zstd"                  => Compression::ZSTD(Default::default()),
        "lz4"                   => Compression::LZ4,
        "none" | "uncompressed" => Compression::UNCOMPRESSED,
        other => return Err(PyRuntimeError::new_err(format!(
            "Unknown compression codec '{}'. Options: snappy, gzip, brotli, zstd, lz4, none",
            other
        ))),
    };

    // 2. Prepare pipeline configuration
    let config = PipelineConfig {
        input_path,
        output_path,
        batch_size,
        compression: comp,
        num_threads: threads,
        ignore_errors,
        flatten,
        limit,
        schema_path: schema,
        channel_depth,
        chunk_size,
    };

    // 3. Release the GIL and run the Rust pipeline
    let result = py.allow_threads(move || run_pipeline(config));

    // 4. Re-acquire the GIL and process the result
    match result {
        Ok(m) => Ok(ConversionMetrics {
            input_bytes: m.input_bytes,
            output_bytes: m.output_bytes,
            rows_processed: m.rows_processed,
            rows_errored: m.rows_errored,
            threads_used: m.threads_used,
            total_duration_ms: m.total_duration_ms,
            parse_duration_ms: m.parse_duration_ms,
            write_duration_ms: m.write_duration_ms,
        }),
        Err(e) => Err(PyRuntimeError::new_err(format!(
            "Parquet conversion failed: {}",
            e
        ))),
    }
}

/// The parq Python module initialization.
#[pymodule]
fn parq(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_class::<ConversionMetrics>()?;
    Ok(())
}
