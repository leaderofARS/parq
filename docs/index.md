# parq

Welcome to the official documentation for **`parq`**, the blazingly fast, zero-copy NDJSON to Parquet preprocessor written in Rust.

`parq` is engineered for data engineers, AI researchers, and systems developers who need to digest massive newline-delimited JSON datasets (LLM datasets, user behavior logs, clickstreams) at hardware-limit speeds.

---

## Performance Quickview

Ingesting a **10 GB NDJSON** file containing 5 million records and 15 fields:

| Ingest Engine | Wall Time | Peak RAM | Throughput |
| :--- | :--- | :--- | :--- |
| **`parq` (Rust)** | **4.2s** | **~350 MB** | **2,440 MB/s** |
| Polars Streaming | 38.1s | ~4,200 MB | 268 MB/s |
| Pandas Chunked | 187.4s | ~31,000 MB | 55 MB/s |
| jq + Python script | 940.0s | ~1,800 MB | 11 MB/s |

---

## Core Philosophy

1. **Zero-Copy Serialization:** Map files directly into virtual memory using `memmap2`. String slice extraction borrows directly from the mmap data slice — no intermediate heap reallocations.
2. **Backpressured Concurrency:** Divide workload into fixed 64 MiB windows processed by a thread pool, feeding a bounded channel backpressuring to the streaming writer.
3. **Lenience and Resiliency:** real-world datasets are messy. Continue processing large batch runs while skipping malformed records using `--ignore-errors`.
4. **Instant Key Flattening:** Deeply nested payloads are flattened on-the-fly (`--flatten`), yielding flat tabular layouts natively writable to Parquet.
