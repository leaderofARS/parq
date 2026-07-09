# parq

[![CI](https://github.com/leaderofARS/parq/actions/workflows/ci.yml/badge.svg)](https://github.com/leaderofARS/parq/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**One binary. No Python. No JVM. NDJSON → Parquet at memory-mapped speed.**

```bash
cargo install parq
parq --input dataset.jsonl --output dataset.parquet
```

---

## Why parq?

Most NDJSON → Parquet tools are wrappers around Python or JVM runtimes.
They load your entire dataset into memory, run query-planner overhead designed
for *interactive analytics*, then write Parquet as an afterthought.

`parq` does one thing and is engineered entirely around doing it fast:

| Problem with the alternatives | How parq solves it |
|---|---|
| Python tools load the entire file into RAM | `memmap2` maps the file; RAM stays **constant** regardless of file size |
| Pandas/Polars use query-planner overhead | parq has **no dataframe abstraction** — raw bytes → Arrow arrays |
| A single corrupt line aborts the whole job | `--ignore-errors` quarantines bad records to a dead-letter file |
| No audit trail on ingested data | SHA-256 of the raw input is embedded in the **Parquet file footer** |
| Installing Polars requires Python + pip + build tools | parq is a **single static binary** — `cargo install parq`, done |

---

## Install

```bash
# From crates.io (once published)
cargo install parq

# From source
git clone https://github.com/leaderofARS/parq
cd parq
cargo build --release
# binary is at: target/release/parq
```

No Python. No pip. No virtualenv. No JVM.

---

## Quick start

```bash
# Auto-detect CPU cores, Snappy compression
parq --input data.jsonl --output data.parquet

# Zstd compression, 8 threads, explicit schema
parq -i data.jsonl -o data.parquet \
     --schema schema.json \
     --compression zstd \
     --threads 8

# Lenient mode: skip bad records, flatten nested objects, cap at 1M rows
parq -i dirty_training_data.jsonl -o clean.parquet \
     --ignore-errors \
     --flatten \
     --limit 1000000 \
     --verbose

# Infer and print the Arrow schema, then exit
parq -i data.jsonl --infer-schema-only > schema.json

# SIMD-accelerated tokenization (AVX2/SSE4.2 CPUs)
cargo build --release --features simd
./target/release/parq -i data.jsonl -o data.parquet
```

---

## Benchmarks

> Measured on a **developer machine** using `cargo bench` (release profile,
> `opt-level=3`, `lto=fat`).  
> Workload: synthetic 4-column NDJSON (`id`, `user_id`, `score`, `active`).  
> These are **single-threaded** `parse_chunk` numbers — the full pipeline
> parallelises across all cores via Rayon.

| Benchmark | Rows | Median time | Throughput |
|---|---|---|---|
| `parse_chunk` | 10 000 | 5.4 ms | **110 MiB/s / core** |
| `parse_chunk` | 100 000 | 52.3 ms | **117 MiB/s / core** |
| `parse_chunk` | 500 000 | 255 ms | **123 MiB/s / core** |
| `schema_infer` | 100 sampled | 67 µs | — |
| `schema_infer` | 1 000 sampled | 657 µs | — |
| `schema_infer` | 10 000 sampled | 6.6 ms | — |
| chunk partitioner | 500 000 rows | **44–99 ns** | O(1) pointer scan |

The parse throughput scales linearly with thread count on independent chunks —
a 4-core machine sees ~400–500 MiB/s end-to-end. Run `cargo bench` to
measure on your own hardware.

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
│   O(1) pointer scan — snaps boundaries to nearest \n             │
└───────┬────────────┬────────────┬────────────┬───────────────────┘
        │ rayon      │ rayon      │ rayon      │ rayon
        ▼            ▼            ▼            ▼
  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
  │  Parse   │ │  Parse   │ │  Parse   │ │  Parse   │
  │ (scalar  │ │ or SIMD  │ │  JSON)   │ │ → Arrow  │
  │ → Batch) │ │ → Batch) │ │ → Batch) │ │  Batch)  │
  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘
       └────────────┴────────────┴────────────┘
                         │
                         ▼  crossbeam bounded channel (depth=8)
                  ┌─────────────┐   ← blocks Rayon when disk is saturated
                  │ Writer      │     (constant RAM regardless of file size)
                  │ ArrowWriter │
                  │ → Parquet   │
                  │ + SHA-256   │   ← provenance hash in Parquet footer
                  └─────────────┘
```

**Key design invariants:**
- The bounded channel is the backpressure valve — RAM stays flat even on 100 GB files
- Each Rayon thread gets a non-overlapping `&[u8]` slice; the borrow checker statically prevents data races
- The only `unsafe` in the entire codebase is the initial `mmap` call

---

## Features

### Resilient ingestion — `--ignore-errors`

```bash
parq -i dirty.jsonl -o clean.parquet --ignore-errors --dead-letter bad.jsonl
```

Malformed lines are logged via `tracing::warn!` and written to the dead-letter
file. The pipeline continues. A single corrupt record in a 50 GB training set
will not abort a multi-hour job.

### Nested JSON flattening — `--flatten`

```
Input:  {"id": 1, "user": {"name": "Alice", "score": 0.9}, "tags": ["a","b"]}
Output: {"id": 1, "user_name": "Alice", "user_score": 0.9, "tags": "[\"a\",\"b\"]"}
```

Depth-first recursive traversal, underscore-joined key paths.

### Explicit schema — `--schema`

```json
[
  { "name": "id",      "type": "Int64",   "nullable": false },
  { "name": "user_id", "type": "Utf8",    "nullable": true  },
  { "name": "score",   "type": "Float64", "nullable": true  },
  { "name": "active",  "type": "Boolean", "nullable": true  }
]
```

Supported types: `Boolean` `Int8` `Int16` `Int32` `Int64` `UInt8` `UInt16`
`UInt32` `UInt64` `Float32` `Float64` `Date32` `Timestamp` `Utf8`

Without `--schema`, parq samples the first N rows and infers the Arrow schema
automatically (type promotion: `Null → Bool → Int64 → Float64 → Utf8`).

### SHA-256 provenance

Every output Parquet file has the SHA-256 hash of the raw input bytes embedded
in its file-level metadata footer under the key `parq.sha256`. Verify it later:

```bash
# The hash is printed in verbose mode
parq -i data.jsonl -o data.parquet -v

# parq.sha256 = a3f9c1...
```

### SIMD tokenization — `--features simd`

```bash
cargo build --release --features simd
```

Replaces `serde_json`'s scalar tokenizer with `simd-json`, which processes JSON
bytes 32 at a time using AVX2/SSE4.2 SIMD instructions. Enabled at compile
time; no runtime overhead on non-SIMD builds. Requires a mutable copy of each
line buffer (the cost is small relative to the SIMD tokenisation gain on
wide schemas).

---

## CLI Reference

```
OPTIONS:
  -i, --input  <FILE>       Input NDJSON file
  -o, --output <FILE>       Output Parquet file
  -c, --compression <CODEC> snappy|gzip|brotli|zstd|lz4|none  [default: snappy]
  -t, --threads <N>         Worker threads  (0 = auto-detect cores) [default: 0]
      --batch-size <N>      Rows per RecordBatch               [default: 65536]
      --channel-depth <N>   Parse→write channel capacity       [default: 8]
      --ignore-errors       Lenient: skip malformed lines (logs to stderr)
      --flatten             Flatten nested objects: user.id → user_id
      --limit <N>           Stop after N rows
      --schema <FILE>       Explicit Arrow schema JSON file
      --chunk-size <BYTES>  Raw chunk size override            [default: 67108864]
      --infer-schema-only   Print inferred schema as JSON and exit
  -v, --verbose             -v info / -vv debug / -vvv trace
  -q, --quiet               Suppress logs (metrics only to stdout as JSON)
  -h, --help
  -V, --version
```

---

## Library usage

### Rust

```toml
[dependencies]
parq-core = { git = "https://github.com/leaderofARS/parq" }
```

```rust
use parq_core::{run_pipeline, PipelineConfig};
use parquet::basic::Compression;

fn main() -> anyhow::Result<()> {
    let metrics = run_pipeline(PipelineConfig {
        input_path:    "data.jsonl".into(),
        output_path:   "data.parquet".into(),
        compression:   Compression::ZSTD(Default::default()),
        num_threads:   0,        // auto-detect cores
        batch_size:    65_536,
        ignore_errors: true,     // resilient mode for dirty data
        flatten:       false,
        channel_depth: 8,
        ..Default::default()
    })?;
    println!("{}", metrics);
    Ok(())
}
```

### Python (via PyO3 / Maturin)

The FFI releases the GIL — `parq.convert()` runs concurrently with Python
threads without blocking the event loop.

```bash
cd crates/parq-python
pip install maturin
maturin develop --release
```

```python
import parq

metrics = parq.convert(
    input_path="large_dataset.jsonl",
    output_path="output.parquet",
    compression="zstd",
    threads=0,            # auto-detect
    ignore_errors=True,
    flatten=True,
)
print(f"{metrics.rows_processed:,} rows in {metrics.total_duration_ms} ms")
print(f"{metrics.input_bytes / 1024**2 / (metrics.total_duration_ms / 1000):.1f} MiB/s")
```

See [MATURIN.md](MATURIN.md) for wheel packaging and PyPI publishing via OIDC.

---

## Memory safety

The only `unsafe` in the codebase is the initial mmap:

```rust
// SAFETY: We hold an exclusive read-only File handle.
// No external process writes to this file for the pipeline lifetime.
let mmap = unsafe { MmapOptions::new().map(&input_file)? };
```

After that the type system takes over:

- `fixed_chunks<'a>(data: &'a [u8]) -> Vec<&'a [u8]>` — the lifetime `'a`
  statically binds every sub-slice to the `Mmap`. Accessing a slice after
  the mmap drops is a **compile error**, not a runtime crash.
- `RecordBatch` transfers ownership across the channel boundary; no `Arc<Mutex<_>>`
  on the hot parse path.
- Rayon's `par_iter` requires `T: Send + Sync`; overlapping mutable access is
  statically rejected.

---

## Roadmap

- [x] `--features simd` — `simd-json` AVX2/SSE4.2 tokenization path
- [ ] `--select col1,col2` — column projection pushdown (skip unwanted keys at parse time)
- [ ] `--features async` — `object_store` I/O for `s3://` and `gs://` sources
- [ ] `--reverse` — Parquet → NDJSON conversion
- [ ] `--format csv` — CSV source support
- [ ] Progress bar (`indicatif` dep already declared)
- [ ] Checkpoint / resume for interrupted jobs

---

## Contributing

The codebase is intentionally small and modular — each concern lives in its
own crate under `crates/`:

| Crate | Responsibility |
|---|---|
| `parq-chunk` | Chunk partitioner (O(1) pointer scan) |
| `parq-parser` | NDJSON → Arrow RecordBatch (scalar + SIMD paths) |
| `parq-schema` | Schema inference and JSON schema loading |
| `parq-io` | Parquet writer + SHA-256 provenance footer |
| `parq-core` | Pipeline orchestrator (mmap → chunks → parse → write) |
| `parq-metrics` | Timing and throughput metrics |
| `parq-error` | Typed error enum |
| `parq-python` | PyO3 FFI bindings |
| `parq` | CLI binary (Clap) |

Run the full check suite locally before opening a PR:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
cargo bench  # optional — generates HTML report at target/criterion/report/
```

---

## License

MIT — see [LICENSE](LICENSE).
