# FFI / Python Bindings

`parq` is distributable as a native Python library built with PyO3. The Python package is engineered to release the Python **Global Interpreter Lock (GIL)**, allowing true concurrent computing on multi-threaded Python systems.

---

## 1. Quick Example

```python
import parq
import threading

def process_logs():
    try:
        # GIL is released during parsing.
        # Python memory overhead remains constant (~0 MB).
        metrics = parq.convert(
            input_path="behavior_logs.jsonl",
            output_path="processed_logs.parquet",
            compression="zstd",
            threads=8,
            ignore_errors=True,
            flatten=True
        )
        print(f"Success! Converted {metrics.rows_processed:,} rows.")
    except RuntimeError as e:
        print(f"Failed: {e}")

# Can execute concurrently alongside other Python tasks
t = threading.Thread(target=process_logs)
t.start()
```

---

## 2. API Signature

### `parq.convert`

```python
def convert(
    input_path: str,
    output_path: str,
    compression: str = "snappy",
    threads: int = 0,
    batch_size: int = 65536,
    channel_depth: int = 8,
    ignore_errors: bool = False,
    flatten: bool = False,
    limit: Optional[int] = None,
    schema: Optional[str] = None,
    chunk_size: Optional[int] = None
) -> ConversionMetrics:
```

* **`compression`**: One of `snappy`, `gzip`, `brotli`, `zstd`, `lz4`, or `none`.
* **`threads`**: Max worker threads. `0` auto-detects based on system CPU.
* **`ignore_errors`**: Resilient mode. Skips and logs malformed lines instead of panicking.
* **`flatten`**: Recursively flattens nested JSON dictionaries into flat column entries.

---

## 3. `parq.ConversionMetrics`

Returned by `convert`. Exposes timing and ingestion statistics:

* **`input_bytes`**: Raw size of input JSON.
* **`output_bytes`**: Compression size of written Parquet.
* **`rows_processed`**: Number of successfully written rows.
* **`rows_errored`**: Number of skipped malformed rows.
* **`threads_used`**: Number of Rayon worker threads deployed.
* **`total_duration_ms`**: Pipeline execution time.
* **`parse_duration_ms`**: CPU parse loop duration.
* **`write_duration_ms`**: Disk write loop duration.
