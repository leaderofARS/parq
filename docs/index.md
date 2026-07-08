# parq — High-Performance NDJSON to Parquet Ingestion Engine

Welcome to the documentation for **`parq`**, a highly specialized, zero-copy, multi-threaded preprocessor written in Rust, designed to convert newline-delimited JSON (NDJSON) files to compressed Apache Parquet.

In modern AI engineering, ingestion is the first and often slowest bottleneck. Large Language Model (LLM) training datasets, massive clickstreams, and data lake raw inputs are frequently distributed in massive NDJSON formats. General-purpose DataFrame engines like Pandas and Polars have query planners, expression compilation, and general-purpose abstractions that introduce memory copy overhead. `parq` is a single-purpose ingestion engine designed to run at the absolute limit of your hardware.

---

## ⚡ Design Goals

`parq` is built with a singular design target: to maximize hardware utilization when transferring newline-delimited JSON to the Apache Parquet format. By avoiding intermediate general-purpose representation steps (like dataframes, query planning, or memory allocation pools), it ensures:
* **Minimal memory footprints** through bounded backpressure channels.
* **Low CPU latency** through zero-copy tokenization.
* **Resiliency** under dirty real-world datasets with a dead-letter quarantine option.

---

## 🚀 Key Features

* **Zero-Copy Ingestion**: Employs OS-level memory mapping (`memmap2`) and borrows string values (`&str`) directly from the memory region without heap allocation or copying.
* **Flow Control / Backpressure**: Uses a bounded `crossbeam` channel between the Rayon parser pool and the streaming writer, keeping memory consumption fixed regardless of file size.
* **Dead-Letter Quarantine**: Lenient mode (`--ignore-errors`) routes malformed records to a quarantined dead-letter file (`--dead-letter-path`) rather than crashing the pipeline.
* **AI-Native Telemetry**: Provides structured JSON error outputs on `stderr` via `--machine-telemetry` for integration with autonomous AI coding agents or orchestration engines.
* **Auto-Schema Inference**: Automatically detects schema types and performs safe type promotion (e.g. `Null -> Boolean -> Int64 -> Float64 -> Utf8`).
* **Nested Object Flattening**: Deeply nested JSON structures can be flattened recursively (`--flatten`) on-the-fly.

---

## 📦 Quick Start

### Installation

You can compile `parq` from source or run it as a compiled binary:

```bash
# Clone and build the release binary
git clone https://github.com/leaderofARS/parq.git
cd parq
cargo build --release

# The compiled binary is located at target/release/parq
```

### CLI Quickstart Examples

```bash
# Basic conversion (auto-detects cores, snappy compression)
parq -i input.jsonl -o output.parquet

# High-performance ZSTD compression using 16 threads and 100k batch size
parq -i dataset.jsonl -o output.parquet -c zstd -t 16 --batch-size 100000

# Lenient mode with a dead-letter quarantine file and automatic nested flattening
parq -i dirty.jsonl -o output.parquet --ignore-errors --flatten --dead-letter-path quarantine.jsonl

# Generate and print the inferred JSON schema to stdout
parq -i input.jsonl --infer-schema-only > schema.json
```
