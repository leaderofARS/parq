//! `parq-io` — Streaming Parquet writer.
//!
//! Accepts `RecordBatch`es one at a time from the bounded channel, writing
//! each directly to disk.  RAM usage is O(`batch_size`) regardless of file size.

use std::{fs::File, path::Path, sync::Arc};

use anyhow::Result;
use arrow::{datatypes::Schema, record_batch::RecordBatch};
use parquet::{
    arrow::ArrowWriter,
    basic::Compression,
    file::properties::WriterProperties,
};
use tracing::debug;

/// A streaming Parquet sink that accepts one `RecordBatch` at a time.
pub struct ParquetStreamWriter {
    inner:          ArrowWriter<File>,
    batches_written: usize,
    rows_written:    usize,
}

impl ParquetStreamWriter {
    /// Open `path` and initialise the Parquet file header.
    pub fn new(path: &Path, schema: Arc<Schema>, compression: Compression, provenance_hash: Option<String>) -> Result<Self> {
        let file  = File::create(path)?;
        let mut builder = WriterProperties::builder()
            .set_compression(compression)
            .set_max_row_group_size(1_048_576)
            .set_created_by("parq/0.2.0".to_string());

        if let Some(hash) = provenance_hash {
            use parquet::file::metadata::KeyValue;
            let unix_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_string());

            builder = builder.set_key_value_metadata(Some(vec![
                KeyValue::new("parq.provenance.sha256".to_string(), hash),
                KeyValue::new("parq.provenance.timestamp".to_string(), unix_ts),
            ]));
        }

        let props = builder.build();
        Ok(Self {
            inner: ArrowWriter::try_new(file, schema, Some(props))?,
            batches_written: 0,
            rows_written:    0,
        })
    }

    /// Write one batch to the Parquet file.
    pub fn write_batch(&mut self, batch: &RecordBatch) -> Result<()> {
        self.inner.write(batch)?;
        self.batches_written += 1;
        self.rows_written    += batch.num_rows();
        debug!(batch = self.batches_written, rows = self.rows_written, "Batch written");
        Ok(())
    }

    /// Flush, write the Parquet footer, and close the file.
    pub fn close(self) -> Result<()> {
        let meta = self.inner.close()?;
        debug!(
            row_groups  = meta.row_groups.len(),
            total_rows  = meta.row_groups.iter().map(|rg| rg.num_rows).sum::<i64>(),
            "Parquet closed"
        );
        Ok(())
    }
}

/// Supported compression codec names (for `--help` output).
pub const COMPRESSION_OPTIONS: &[&str] = &["snappy", "gzip", "brotli", "zstd", "lz4", "none"];

/// Parse a codec name → `parquet::basic::Compression`.
pub fn parse_compression(s: &str) -> Result<Compression> {
    Ok(match s.to_lowercase().as_str() {
        "snappy"                => Compression::SNAPPY,
        "gzip"                  => Compression::GZIP(Default::default()),
        "brotli"                => Compression::BROTLI(Default::default()),
        "zstd"                  => Compression::ZSTD(Default::default()),
        "lz4"                   => Compression::LZ4,
        "none" | "uncompressed" => Compression::UNCOMPRESSED,
        other => anyhow::bail!(
            "Unknown compression '{}'. Options: {:?}", other, COMPRESSION_OPTIONS
        ),
    })
}
