//! `parq-flatten` — Recursive JSON object flattening.
//!
//! Converts `{"user": {"id": 1, "name": "Alice"}}` into
//! `{"user_id": 1, "user_name": "Alice"}` by depth-first traversal,
//! joining nested key paths with a configurable separator.
//!
//! Arrays are serialised as JSON strings — Parquet's native LIST type
//! requires a schema-time declaration, so free-form arrays are stored as
//! `Utf8` by default. Use an explicit `--schema` file with `List` columns
//! if you need typed arrays.

use serde_json::{Map, Value};

/// Recursively flatten a JSON object into a single-level `Map`.
/// Nested keys are joined with `separator` (typically `"_"`).
///
/// # Example
/// ```
/// use parq_flatten::flatten_object;
/// use serde_json::json;
///
/// let obj = json!({"a": {"b": 1}, "c": 2});
/// let flat = flatten_object(obj.as_object().unwrap(), "_");
/// assert_eq!(flat["a_b"], json!(1));
/// assert_eq!(flat["c"],   json!(2));
/// ```
pub fn flatten_object(obj: &Map<String, Value>, separator: &str) -> Map<String, Value> {
    let mut out = Map::new();
    flatten_rec(obj, "", separator, &mut out);
    out
}

fn flatten_rec(obj: &Map<String, Value>, prefix: &str, sep: &str, out: &mut Map<String, Value>) {
    for (key, value) in obj {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}{sep}{key}")
        };
        match value {
            Value::Object(nested) => flatten_rec(nested, &full, sep, out),
            Value::Array(_) => {
                out.insert(full, Value::String(value.to_string()));
            }
            scalar => {
                out.insert(full, scalar.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flat(v: serde_json::Value) -> Map<String, Value> {
        flatten_object(v.as_object().unwrap(), "_")
    }

    #[test]
    fn flat_object_unchanged() {
        let f = flat(json!({"a": 1, "b": "x", "c": true}));
        assert_eq!(f["a"], json!(1));
        assert_eq!(f["b"], json!("x"));
        assert_eq!(f["c"], json!(true));
    }

    #[test]
    fn single_level_nesting() {
        let f = flat(json!({"user": {"id": 42, "name": "Alice"}}));
        assert_eq!(f["user_id"], json!(42));
        assert_eq!(f["user_name"], json!("Alice"));
        assert!(!f.contains_key("user"));
    }

    #[test]
    fn deep_nesting() {
        let f = flat(json!({"a": {"b": {"c": {"d": 99}}}}));
        assert_eq!(f["a_b_c_d"], json!(99));
    }

    #[test]
    fn array_becomes_json_string() {
        let f = flat(json!({"tags": ["rust", "parquet"]}));
        assert_eq!(f["tags"], json!(r#"["rust","parquet"]"#));
    }

    #[test]
    fn null_preserved() {
        let f = flat(json!({"x": null, "n": {"y": null}}));
        assert_eq!(f["x"], Value::Null);
        assert_eq!(f["n_y"], Value::Null);
    }
}
