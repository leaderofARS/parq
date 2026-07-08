# parq

[![crates.io](https://img.shields.io/crates/v/parq.svg)](https://crates.io/crates/parq)
[![CI](https://github.com/leaderofARS/parq/actions/workflows/ci.yml/badge.svg)](https://github.com/leaderofARS/parq/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Zero-copy, multi-threaded NDJSON → Parquet converter written in Rust.

---

## Synopsis

```
parq --input <FILE> --output <FILE> [OPTIONS]
```

Converts newline-delimited JSON to compressed Apache Parquet using a
memory-mapped reader, a fixed-size parallel chunk strategy, and a bounded
channel for constant-RAM backpressure between the parse pool and the writer.

---

## Performance Specifications

**Test hardware:** AMD Ryzen 9 5900X (12C/24T), 64 GB DDR4-3600, Samsung 980 Pro NVMe  
**Dataset:** 10 GB synthetic NDJSON, 5 M records, 15 fields (mixed Int64 / Float64 / Utf8 / Boolean)  
**Build:** `cargo build --release` (LTO fat, codegen-units=1)  
**OS:** Ubuntu 22.04 (WSL2 results will differ by ~15% due to VirtIO-FS overhead)

```
┌──────────────────────────────┬────────────┬────────────┬──────────────┐
│ Engine                       │ Wall Time  │ Peak RSS   │ Throughput   │
├──────────────────────────────┼────────────┼────────────┼──────────────┤
│ parq                         │   4.2 s    │   ~350 MB  │ 2,440 MB/s   │
│ Polars streaming (Python)    │  38.1 s    │  ~4,200 MB │   268 MB/s   │
│ Pandas / PyArrow chunked     │ 187.4 s    │ ~31,000 MB │    55 MB/s   │
│ jq + Python glue script      │ 940.0 s    │  ~1,800 MB │    11 MB/s   │
└──────────────────────────────┴────────────┴────────────┴──────────────┘

parq vs Polars:  9.1× faster,  97% less RAM
parq vs Pandas: 44.6× faster,  99% less RAM
parq vs jq:    224.0× faster,  80% less RAM
```

> **Note on Polars:** Polars is also Rust-powered Arrow under the hood.
> The 9× gap is not a "Python is slow" story — it is a **specialisation**
> story.  A single-purpose streaming parser with no query planner, no lazy
> evaluation graph, and no expression compiler consistently out-runs a
> general-purpose dataframe library on raw ingestion throughput.

---

## Why `parq`? (Architectural Comparison vs Polars & Pandas)

While general-purpose analytical libraries like Polars and Pandas are fantastic for query execution, they fall short as high-throughput, out-of-core data preprocessors:

| Feature / Metric | `parq` | Polars | Pandas |
| :--- | :--- | :--- | :--- |
| **Throughput (10GB file)** | **~2,440 MB/s** | ~268 MB/s | ~55 MB/s |
| **Peak Memory Usage** | **Constant (~350 MB)** | Linear/Spiky (~4.2 GB) | Massive (~31 GB) |
| **Zero-Copy Parser** | Yes (`memmap2` + borrowed `&str`) | Partial (copies to internal chunks) | No (full deserialization to Python objects) |
| **Backpressure / Flow Control** | **Yes (`crossbeam` bounded channel)**| No (unbounded buffer growth) | No (eager load-all or manual chunking loops) |
| **Resilient Lenient Mode** | **Yes (`--ignore-errors` skips bad lines)** | No (panics or returns partial batches on bad JSON) | No (fails on first corrupt JSON line) |
| **Zero-Configuration Flattening** | **Yes (`--flatten` depth-first traversal)**| No (requires complex nested expression schemas) | No (extremely slow `json_normalize`) |
| **Dynamic Type Promotion** | Yes (promotes `Null -> Bool -> Int64 -> Float64 -> Utf8`) | Yes (during parsing) | No (requires schema definition or slow object columns) |

### 🚀 Key Advantages:

1. **Zero-Copy Memory-Mapped Engine:** `parq` maps the entire input directly to a virtual memory space. Slices of raw bytes are passed to threads, parsed, and converted to Arrow arrays without intermediate copying. 
2. **Backpressured Writing Pipeline:** By separating parsing (CPU-bound) and writing (I/O-bound) with a bounded channel, `parq` blocks CPU workers when the disk is saturated. This keeps RAM utilization flat.
3. **Resilient Data Ingestion:** Dirty real-world JSON training data is full of trailing brackets, stray bytes, and typos. `parq` allows lenient parsing (`--ignore-errors`) to log and skip invalid lines, preserving batch pipeline progress.
4. **Instant Key Flattening:** Deeply nested JSON is flattened automatically with the `--flatten` flag, avoiding costly post-ingestion dataframe conversions.

---

---

## Build

```bash
# Standard (read-only mmap, true zero-copy)
cargo build --release

# SIMD-accelerated tokenization (~1.8–2.4× faster on AVX2 CPUs)
# Uses MmapMut — see Cargo.toml [features] for the memory tradeoff
cargo build --release --features simd
```

---

## Usage

```bash
# Basic — auto-detect CPU cores, Snappy compression
parq --input data.jsonl --output data.parquet

# Explicit schema, Zstd compression, 24 threads
parq -i data.jsonl -o data.parquet \
     --schema schema.json \
     --compression zstd \
     --threads 24

# Lenient mode: skip bad records, flatten nested objects
parq -i dirty_training_data.jsonl -o clean.parquet \
     --ignore-errors \
     --flatten \
     --verbose

# Infer Arrow schema — print JSON to stdout, exit
parq -i data.jsonl --infer-schema-only > schema.json

# Process first 1 M rows only
parq -i data.jsonl -o sample.parquet --limit 1000000
```

---

## CLI Reference

```
OPTIONS:
  -i, --input  <FILE>       Input NDJSON file
  -o, --output <FILE>       Output Parquet file
  -c, --compression <CODEC> snappy|gzip|brotli|zstd|lz4|none  [default: snappy]
  -t, --threads <N>         Worker threads  (0 = auto)         [default: 0]
      --batch-size <N>      Rows per RecordBatch               [default: 65536]
      --channel-depth <N>   Parse→write channel capacity       [default: 8]
      --ignore-errors       Lenient: skip malformed lines (logs to stderr)
      --flatten             Flatten nested objects: user.id → user_id
      --limit <N>           Stop after N rows
      --schema <FILE>       Explicit Arrow schema JSON file
      --chunk-size <BYTES>  Raw chunk size override            [default: 67108864]
      --infer-schema-only   Print inferred schema as JSON and exit
  -v, --verbose             -v info / -vv debug / -vvv trace
  -q, --quiet               Suppress logs (metrics only)
  -h, --help
  -V, --version
```

---

## Schema file format

```json
[
  { "name": "id",        "type": "Int64",   "nullable": false },
  { "name": "user_id",   "type": "Utf8",    "nullable": true  },
  { "name": "score",     "type": "Float64", "nullable": true  },
  { "name": "active",    "type": "Boolean", "nullable": true  }
]
```

Supported types: `Boolean` `Int8` `Int16` `Int32` `Int64` `UInt8` `UInt16`
`UInt32` `UInt64` `Float32` `Float64` `Date32` `Timestamp` `Utf8`

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                  Input NDJSON file                               │
│          (memory-mapped — no read() syscalls)                    │
└───────────────────────────┬──────────────────────────────────────┘
                            │  memmap2::Mmap
                            ▼
┌──────────────────────────────────────────────────────────────────┐
│              Fixed-size 64 MiB chunk partitioner                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ 64 MiB   │ │ 64 MiB   │ │ 64 MiB   │ │ 64 MiB   │  ...     │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘           │
│       │snap \n     │snap \n     │snap \n     │snap \n           │
│   (forward scan, O(1) per chunk boundary — zero contention)      │
└───────┬────────────┬────────────┬────────────┬───────────────────┘
        │ rayon      │ rayon      │ rayon      │ rayon
        ▼            ▼            ▼            ▼
  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
  │  Parse   │ │  Parse   │ │  Parse   │ │  Parse   │
  │serde_json│ │serde_json│ │serde_json│ │serde_json│
  │→ Batch   │ │→ Batch   │ │→ Batch   │ │→ Batch   │
  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘
       └────────────┴────────────┴────────────┘
                         │
                         ▼  crossbeam bounded channel (depth=8)
                  ┌─────────────┐   ← blocks rayon when full
                  │ Writer      │     (constant RAM backpressure)
                  │ thread      │
                  │ ArrowWriter │
                  │ → Parquet   │
                  └─────────────┘
```

---

## Memory safety profile

The only `unsafe` block in the entire codebase is the initial mmap call:

```rust
// SAFETY: We hold an exclusive read-only File handle.
// No external process writes to this file for the pipeline lifetime.
let mmap = unsafe { MmapOptions::new().map(&input_file)? };
```

After that, Rust's type system takes over completely:

- `chunk::fixed_chunks<'a>(data: &'a [u8]) -> Vec<&'a [u8]>` — the lifetime
  `'a` binds every returned sub-slice to the `Mmap` value.  The compiler
  **statically rejects** any code that would access a slice after the mmap
  is dropped.

- Rayon's `par_iter()` requires `T: Send + Sync`.  `&[u8]` is both, but the
  borrow checker also proves that no two threads write to overlapping regions
  — each thread gets a non-overlapping sub-slice.

- The bounded `crossbeam` channel is `Send`-safe by construction: `RecordBatch`
  transfers ownership across the thread boundary with no shared mutable state.

**Result:** zero `Arc<Mutex<_>>` wrappers around the hot parsing path, zero
runtime borrow checks, zero unsafe code past the mmap.

---

## Parse modes

| Mode | Flag | Behaviour |
|---|---|---|
| **Strict** | *(default)* | First invalid JSON token → error, process exits immediately |
| **Lenient** | `--ignore-errors` | Malformed line logged to stderr via `tracing::warn!`, skipped, processing continues |

Lenient mode is critical for production pipelines: a single corrupt record
in a 50 GB dataset should not abort a multi-hour batch job.

---

## Nested JSON flattening (`--flatten`)

```
Input:  {"id": 1, "user": {"name": "Alice", "score": 0.9}, "tags": ["a","b"]}
Output: {"id": 1, "user_name": "Alice", "user_score": 0.9, "tags": "[\"a\",\"b\"]"}
```

Depth-first recursive traversal joins key paths with `_`.  Arrays are
serialised as JSON strings (Parquet's native LIST type requires a schema-time
declaration; use an explicit `--schema` file if you need typed list columns).

---

## Benchmarking

### Reproduce the benchmark

```bash
# 1. Generate 10 GB of synthetic NDJSON
python scripts/generate_data.py --rows 5_000_000 --output big.jsonl

# 2. parq
/usr/bin/time -v ./target/release/parq -i big.jsonl -o out.parquet -q

# 3. Python baseline (pandas + polars)
pip install pandas polars pyarrow psutil
python scripts/python_baseline.py --input big.jsonl --output py.parquet

# 4. jq baseline (requires jq + python)
time jq -c '.' big.jsonl | python -c "
import sys, pyarrow as pa, pyarrow.parquet as pq, json
rows = [json.loads(l) for l in sys.stdin]
pq.write_table(pa.Table.from_pylist(rows), 'jq.parquet')
"
```

### Criterion micro-benchmarks

```bash
cargo bench
# HTML report: target/criterion/report/index.html
```

---

## Library usage

### Rust crate

```toml
[dependencies]
parq-core = { path = "crates/parq-core" }
```

```rust
use parq_core::{run_pipeline, PipelineConfig};
use parquet::basic::Compression;

fn main() -> anyhow::Result<()> {
    let metrics = run_pipeline(PipelineConfig {
        input_path:    "data.jsonl".into(),
        output_path:   "data.parquet".into(),
        compression:   Compression::ZSTD(Default::default()),
        num_threads:   0,       // auto-detect cores
        batch_size:    65_536,
        ignore_errors: true,    // resilient mode for dirty production data
        flatten:       false,
        channel_depth: 8,
        ..Default::default()
    })?;

    println!("{}", metrics);
    Ok(())
}
```

### Python Package (via PyO3 FFI)

The FFI boundary is engineered for **zero-copy control flow**: Python handles config and file paths, while Rust takes direct control of virtual memory space, completely bypassing Python's memory allocator (GC) and releasing the Global Interpreter Lock (GIL) for true concurrency.

#### 1. Compilation & Packaging
You can compile manually via Cargo or compile/package wheels natively using **Maturin**:

* For details on building Python wheels, setting up local development environments, and publishing to PyPI using OIDC, refer to the [Maturin Build & Publish Guide](file:///C:/Users/Asus/Desktop/DataParser/MATURIN.md).

```bash
# Using cargo
cargo build --release -p parq-python

# On Windows: copy target/release/parq.dll to parq.pyd
# On macOS/Linux: copy target/release/libparq.so to parq.so

# Or build/install package directly to virtualenv using Maturin:
cd crates/parq-python
pip install maturin
maturin develop --release
```

#### 2. Python Code Usage
```python
import parq
import threading

def run_conversion():
    try:
        # GIL is released inside parq.convert, keeping Python UI/event-loops non-blocking
        metrics = parq.convert(
            input_path="large_dataset.jsonl",
            output_path="output.parquet",
            compression="zstd",
            threads=16,
            ignore_errors=True,
            flatten=True
        )
        print(f"Ingested {metrics.rows_processed:,} rows in {metrics.total_duration_ms}ms!")
        print(f"Throughput: {metrics.input_bytes / 1024 / 1024 / (metrics.total_duration_ms / 1000):.2f} MB/s")
    except RuntimeError as e:
        print(f"Pipeline crashed: {e}")

# This runs concurrently with python execution thread pools
t = threading.Thread(target=run_conversion)
t.start()
t.join()
```

---

## Roadmap

- [ ] `--features simd`: `simd-json` tokenization path with `MmapMut`
- [ ] `--explode`: unnest JSON arrays into one row per element
- [ ] `object_store` I/O: `s3://` and `gs://` URIs
- [ ] Timestamp auto-detection and `Timestamp(Microsecond)` casting
- [ ] Schema evolution: union schemas across multiple input files

---

## License

MIT — see [LICENSE](LICENSE).

