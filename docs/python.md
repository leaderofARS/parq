# Python Bindings & FFI Integration

`parq` compiles as a native C-compatible dynamic library (`cdylib`), exposing a high-performance Python module built on **PyO3**. It is engineered to bypass Python's memory constraints and allow true multi-threaded parallelism.

---

## 1. The FFI Design (GIL Release & GC Bypass)

Standard Python bindings struggle with performance because:
1. **The Global Interpreter Lock (GIL)** prevents Python threads from running concurrently.
2. **Python's Garbage Collector (GC)** incurs massive overhead when managing millions of small objects (like parsed JSON values).

### How `parq` Solves This:
* **GC Bypass**: Python only handles control parameters (input/output file paths, thread counts, and options). It never touches the raw data chunks. Rust's memory mapper reads the file, parses the data, and writes the Parquet output completely inside the Rust heap and system page cache.
* **GIL Release**: Inside the PyO3 wrapper, Rust calls `py.allow_threads` before initiating the ingestion pipeline. This detaches the Python interpreter, allowing other Python threads to execute concurrently (e.g. running event loops, fetching files from S3, or updating web servers).

---

## 2. API Reference

### `parq.convert(...)`

Exposes the core Rust ingestion pipeline.

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
    chunk_size: Optional[int] = None,
    dead_letter_path: Optional[str] = None
) -> ConversionMetrics:
```

#### Arguments:
* **`input_path`** (str): Path to the source NDJSON file.
* **`output_path`** (str): Target path for the written Parquet file.
* **`compression`** (str): Compression codec. Options: `snappy`, `gzip`, `brotli`, `zstd`, `lz4`, `none`.
* **`threads`** (int): Maximum threads to deploy. `0` auto-detects system CPU count.
* **`batch_size`** (int): Row batch size for each Arrow `RecordBatch` block.
* **`channel_depth`** (int): Channel capacity for backpressure control.
* **`ignore_errors`** (bool): Lenient mode. If true, logs and skips corrupt lines instead of raising an exception.
* **`flatten`** (bool): Recursively flattens nested JSON dictionaries into flat column structures (joins paths with `_`).
* **`limit`** (Optional[int]): Stop ingestion after processing this number of records.
* **`schema`** (Optional[str]): Path to an explicit JSON schema file (disables auto-inference).
* **`chunk_size`** (Optional[int]): Override default 64 MiB raw partitioning window size.
* **`dead_letter_path`** (Optional[str]): Write raw malformed JSON records to this path when `ignore_errors` is enabled.

---

## 3. `parq.ConversionMetrics`

Lightweight stats struct returned on success:

```python
class ConversionMetrics:
    input_bytes: int        # Raw input file size (bytes)
    output_bytes: int       # Written Parquet file size (bytes)
    rows_processed: int     # Rows successfully parsed and written
    rows_errored: int       # Rows skipped due to errors
    threads_used: int       # CPU threads deployed
    total_duration_ms: int  # Full pipeline elapsed time
    parse_duration_ms: int  # Hot parse loop time
    write_duration_ms: int  # Parquet write time
```

---

## 4. Multi-Threaded Code Example

This example demonstrates running `parq` inside a Python thread pool. Because the GIL is released during ingestion, multiple conversions can run simultaneously without freezing your main thread:

```python
import parq
import threading
import time

def ingest_file(filename: str, out_name: str):
    print(f"Starting ingestion of {filename}...")
    try:
        metrics = parq.convert(
            input_path=filename,
            output_path=out_name,
            compression="zstd",
            ignore_errors=True,
            dead_letter_path=f"err_{filename}",
            flatten=True
        )
        print(f"Success [{filename}]: {metrics.rows_processed:,} rows in {metrics.total_duration_ms}ms")
    except RuntimeError as e:
        print(f"Ingestion failed for {filename}: {e}")

# Ingest two files concurrently
t1 = threading.Thread(target=ingest_file, args=("dataset_A.jsonl", "out_A.parquet"))
t2 = threading.Thread(target=ingest_file, args=("dataset_B.jsonl", "out_B.parquet"))

t0 = time.perf_counter()
t1.start()
t2.start()

t1.join()
t2.join()
print(f"Total parallel ingestion time: {time.perf_counter() - t0:.2f}s")
```
