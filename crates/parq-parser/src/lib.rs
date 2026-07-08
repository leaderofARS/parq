//! `parq-parser` — Parallel NDJSON → Arrow `RecordBatch` parser.
//!
//! ## Parse modes
//!
//! | Mode    | Flag              | Behaviour                                          |
//! |---------|-------------------|----------------------------------------------------|
//! | Strict  | *(default)*       | First bad token → error, entire pipeline aborts    |
//! | Lenient | `--ignore-errors` | Bad line logged via `tracing::warn!`, skipped      |
//!
//! Lenient mode is critical for production: a single corrupt record in a
//! 50 GB dataset must not abort a multi-hour batch job.

use std::sync::Arc;

use anyhow::Result;
use arrow::{
    array::{ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, NullArray, StringBuilder},
    datatypes::{DataType, Schema},
    record_batch::RecordBatch,
};
use parq_error::ParqError;
use parq_flatten::flatten_object;
use serde_json::{Map, Value};
use tracing::warn;

/// Parse one 64 MiB chunk (many NDJSON lines) into a list of `RecordBatch`es.
///
/// # Returns
/// `(batches, rows_ok, rows_skipped)`
pub fn parse_chunk(
    data: &[u8],
    schema: Arc<Schema>,
    batch_size: usize,
    ignore_errors: bool,
    flatten_json: bool,
    limit: Option<usize>,
    dead_letter_tx: Option<&crossbeam_channel::Sender<Vec<u8>>>,
) -> Result<(Vec<RecordBatch>, usize, usize)> {
    let fields = schema.fields();
    let mut batches = Vec::new();
    let mut builders: Vec<ColBuilder> = fields
        .iter()
        .map(|f| ColBuilder::new(f.data_type(), batch_size))
        .collect();

    let mut batch_rows = 0usize;
    let mut total_ok = 0usize;
    let mut total_skip = 0usize;
    let mut line_no = 0usize;

    for line_bytes in data.split(|&b| b == b'\n') {
        line_no += 1;
        if let Some(lim) = limit {
            if total_ok >= lim {
                break;
            }
        }
        let trimmed = line_bytes.trim_ascii();
        if trimmed.is_empty() {
            continue;
        }

        // ── Parse ────────────────────────────────────────────────────
        let value: Value = match serde_json::from_slice(trimmed) {
            Ok(v) => v,
            Err(e) => {
                if ignore_errors {
                    warn!(line = line_no, error = %e,
                          preview = %String::from_utf8_lossy(&trimmed[..trimmed.len().min(80)]),
                          "Skipping malformed JSON");
                    total_skip += 1;
                    if let Some(tx) = dead_letter_tx {
                        let _ = tx.send(trimmed.to_vec());
                    }
                    continue;
                }
                return Err(ParqError::JsonParse {
                    line: line_no,
                    source: e,
                }
                .into());
            }
        };

        // ── Unwrap to object ─────────────────────────────────────────
        let mut obj: Map<String, Value> = match value {
            Value::Object(m) => m,
            _ => {
                if ignore_errors {
                    warn!(line = line_no, "Expected JSON object — skipping");
                    total_skip += 1;
                    if let Some(tx) = dead_letter_tx {
                        let _ = tx.send(trimmed.to_vec());
                    }
                    continue;
                }
                return Err(ParqError::JsonParse {
                    line: line_no,
                    source: serde_json::from_str::<Value>("!").unwrap_err(),
                }
                .into());
            }
        };

        // ── Optional flatten ─────────────────────────────────────────
        if flatten_json {
            obj = flatten_object(&obj, "_");
        }

        // ── Column dispatch ──────────────────────────────────────────
        for (i, field) in fields.iter().enumerate() {
            let val = obj.get(field.name());
            builders[i].append(val, line_no, ignore_errors)?;
        }

        batch_rows += 1;
        total_ok += 1;

        if batch_rows >= batch_size {
            batches.push(flush(&schema, &mut builders, batch_size)?);
            batch_rows = 0;
        }
    }
    if batch_rows > 0 {
        batches.push(flush(&schema, &mut builders, batch_size)?);
    }
    Ok((batches, total_ok, total_skip))
}

fn flush(
    schema: &Arc<Schema>,
    builders: &mut [ColBuilder],
    batch_size: usize,
) -> Result<RecordBatch> {
    let columns: Vec<ArrayRef> = builders
        .iter_mut()
        .map(|b| b.finish())
        .collect::<Result<_>>()?;
    for (i, field) in schema.fields().iter().enumerate() {
        builders[i] = ColBuilder::new(field.data_type(), batch_size);
    }
    Ok(RecordBatch::try_new(Arc::clone(schema), columns)?)
}

// ── Type-dispatched column builder ────────────────────────────────────────────

enum ColBuilder {
    Null { len: usize },
    Bool(BooleanBuilder),
    Int64(Int64Builder),
    Float64(Float64Builder),
    Str(StringBuilder),
}

impl ColBuilder {
    fn new(dtype: &DataType, cap: usize) -> Self {
        match dtype {
            DataType::Null => Self::Null { len: 0 },
            DataType::Boolean => Self::Bool(BooleanBuilder::with_capacity(cap)),
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => Self::Int64(Int64Builder::with_capacity(cap)),
            DataType::Float32 | DataType::Float64 => {
                Self::Float64(Float64Builder::with_capacity(cap))
            }
            _ => Self::Str(StringBuilder::with_capacity(cap, cap * 24)),
        }
    }

    fn append(&mut self, val: Option<&Value>, line: usize, ignore_errors: bool) -> Result<()> {
        match self {
            Self::Null { len } => *len += 1,

            Self::Bool(b) => match val {
                Some(Value::Bool(v)) => b.append_value(*v),
                Some(Value::Null) | None => b.append_null(),
                Some(other) => {
                    if ignore_errors {
                        b.append_null();
                    } else {
                        return Err(ParqError::TypeMismatch {
                            field: "bool_field".into(),
                            expected: "Boolean".into(),
                            found: format!("{other:?}"),
                            line,
                        }
                        .into());
                    }
                }
            },

            Self::Int64(b) => match val {
                Some(Value::Number(n)) => b.append_value(
                    n.as_i64()
                        .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
                ),
                Some(Value::String(s)) => b.append_option(s.parse::<i64>().ok()),
                Some(Value::Null) | None => b.append_null(),
                Some(_) => b.append_null(),
            },

            Self::Float64(b) => match val {
                Some(Value::Number(n)) => b.append_value(n.as_f64().unwrap_or(f64::NAN)),
                Some(Value::String(s)) => b.append_option(s.parse::<f64>().ok()),
                Some(Value::Null) | None => b.append_null(),
                Some(_) => b.append_null(),
            },

            Self::Str(b) => match val {
                Some(Value::String(s)) => b.append_value(s),
                Some(Value::Null) | None => b.append_null(),
                Some(other) => b.append_value(other.to_string()),
            },
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<ArrayRef> {
        Ok(match self {
            Self::Null { len } => Arc::new(NullArray::new(*len)),
            Self::Bool(b) => Arc::new(b.finish()),
            Self::Int64(b) => Arc::new(b.finish()),
            Self::Float64(b) => Arc::new(b.finish()),
            Self::Str(b) => Arc::new(b.finish()),
        })
    }
}
