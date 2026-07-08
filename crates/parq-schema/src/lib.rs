//! `parq-schema` — Arrow schema inference and explicit loading.
//!
//! ## Type promotion hierarchy
//!
//! Each field's type is promoted upward as wider values are encountered
//! across the sample window:
//!
//! ```text
//!  Null → Boolean → Int64 → Float64 → Utf8   (ceiling)
//! ```

use std::collections::HashMap;

use anyhow::Result;
use arrow::datatypes::{DataType, Field, Schema};
use parq_error::ParqError;
use serde_json::Value;
use tracing::debug;

// ── BOM handling ──────────────────────────────────────────────────────────────

/// Strip a UTF-8 BOM (`EF BB BF`) if present.
/// Windows tools (PowerShell `Out-File`, Notepad) prepend this silently.
#[inline]
fn strip_bom(data: &[u8]) -> &[u8] {
    data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Infer an Arrow `Schema` by scanning up to `sample_size` records.
///
/// Samples from the beginning of `data` (typically the full mmap region).
/// Returns an error only if the file contains zero parseable records.
pub fn infer_schema(data: &[u8], sample_size: usize) -> Result<Schema> {
    let data = strip_bom(data);
    let mut field_types: HashMap<String, DataType> = HashMap::new();
    let mut field_order: Vec<String> = Vec::new();
    let mut rows_seen = 0usize;

    for line in data.split(|&b| b == b'\n').take(sample_size) {
        let trimmed = line.trim_ascii();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_slice(trimmed).map_err(|e| ParqError::JsonParse {
                line: rows_seen + 1,
                source: e,
            })?;

        if let Value::Object(obj) = value {
            for (key, val) in &obj {
                let entry = field_types.entry(key.clone()).or_insert(DataType::Null);
                let promoted = promote(entry, val);
                if *entry == DataType::Null && promoted != DataType::Null {
                    field_order.push(key.clone());
                }
                *entry = promoted;
            }
        }
        rows_seen += 1;
    }

    if rows_seen == 0 {
        return Err(ParqError::InsufficientData { rows: 0 }.into());
    }
    debug!(sampled = rows_seen, "Schema inferred");

    let fields: Vec<Field> = field_order
        .iter()
        .map(|name| {
            let dtype = field_types.get(name).cloned().unwrap_or(DataType::Utf8);
            Field::new(name, dtype, true)
        })
        .collect();

    Ok(Schema::new(fields))
}

/// Load a schema from a JSON file in parq's schema format.
///
/// Format:
/// ```json
/// [
///   { "name": "id",    "type": "Int64",   "nullable": false },
///   { "name": "score", "type": "Float64", "nullable": true  }
/// ]
/// ```
pub fn load_schema_from_file(path: &str) -> Result<Schema> {
    let content = std::fs::read_to_string(path)?;
    let content_clean = content.strip_prefix("\u{feff}").unwrap_or(&content);
    let entries: Vec<Value> = serde_json::from_str(content_clean)?;
    let fields: Vec<Field> = entries
        .iter()
        .map(|e| {
            let name     = e["name"].as_str().unwrap_or("unknown").to_string();
            let dtype    = str_to_datatype(e["type"].as_str().unwrap_or("Utf8"));
            let nullable = e["nullable"].as_bool().unwrap_or(true);
            Field::new(name, dtype, nullable)
        })
        .collect();
    Ok(Schema::new(fields))
}

/// Serialise a `Schema` to parq's JSON schema format (for `--infer-schema-only`).
pub fn schema_to_json_string(schema: &Schema) -> Result<String> {
    let fields: Vec<Value> = schema
        .fields()
        .iter()
        .map(|f| serde_json::json!({
            "name":     f.name(),
            "type":     datatype_to_str(f.data_type()),
            "nullable": f.is_nullable(),
        }))
        .collect();
    Ok(serde_json::to_string_pretty(&fields)?)
}

// ── Type helpers ──────────────────────────────────────────────────────────────

fn promote(current: &DataType, value: &Value) -> DataType {
    merge(current, &value_type(value))
}

fn value_type(v: &Value) -> DataType {
    match v {
        Value::Null       => DataType::Null,
        Value::Bool(_)    => DataType::Boolean,
        Value::Number(n)  => if n.is_i64() { DataType::Int64 } else { DataType::Float64 },
        _                 => DataType::Utf8,
    }
}

fn merge(a: &DataType, b: &DataType) -> DataType {
    if a == b { return a.clone(); }
    use DataType::*;
    match (a, b) {
        (Null, x) | (x, Null)              => x.clone(),
        (Boolean, Int64)   | (Int64, Boolean)   => Int64,
        (Boolean, Float64) | (Float64, Boolean) => Float64,
        (Int64,   Float64) | (Float64, Int64)   => Float64,
        (Utf8, _) | (_, Utf8)               => Utf8,
        _                                    => Utf8,
    }
}

pub fn str_to_datatype(s: &str) -> DataType {
    match s {
        "Boolean"   => DataType::Boolean,
        "Int8"      => DataType::Int8,   "Int16"  => DataType::Int16,
        "Int32"     => DataType::Int32,  "Int64"  => DataType::Int64,
        "UInt8"     => DataType::UInt8,  "UInt16" => DataType::UInt16,
        "UInt32"    => DataType::UInt32, "UInt64" => DataType::UInt64,
        "Float32"   => DataType::Float32,"Float64"=> DataType::Float64,
        "Date32"    => DataType::Date32,
        "Timestamp" => DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
        _           => DataType::Utf8,
    }
}

pub fn datatype_to_str(dt: &DataType) -> &'static str {
    match dt {
        DataType::Boolean => "Boolean", DataType::Int8    => "Int8",
        DataType::Int16   => "Int16",   DataType::Int32   => "Int32",
        DataType::Int64   => "Int64",   DataType::UInt8   => "UInt8",
        DataType::UInt16  => "UInt16",  DataType::UInt32  => "UInt32",
        DataType::UInt64  => "UInt64",  DataType::Float32 => "Float32",
        DataType::Float64 => "Float64", DataType::Date32  => "Date32",
        _                 => "Utf8",
    }
}
