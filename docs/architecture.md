# System Architecture & Internals

`parq` achieves hardware-limit throughput by eliminating general-purpose runtime abstractions and focusing on zero-copy memory pipelines. This document describes the low-level systems engineering designs that power the engine.

---

## 1. Zero-Copy OS Memory-Mapping

In standard parsing pipelines, the OS reads file bytes into a kernel buffer, copies them to user space via `read()` system calls, parses those bytes into intermediate structs (e.g. `serde_json::Value` on the heap), and finally copies them into Arrow column buffers. This creates massive memory bandwidth saturation and garbage collection overhead.

`parq` maps the entire input file directly into the process's virtual address space using `memmap2`:

```
┌────────────────────────────────────────────────────────┐
│                   VIRTUAL ADDRESS SPACE                │
│ ┌────────────────────────────────────────────────────┐ │
│ │                  Mapped Input File                 │ │
│ │ [line 1\n][line 2\n][line 3\n][line 4\n] ...       │ │
│ └────────────────────────────────────────────────────┘ │
└───────────────────────────┬────────────────────────────┘
                            │
              Zero-Copy Lifetimes (&'a str)
                            ▼
┌────────────────────────────────────────────────────────┐
│                   ARROW BUFFER POOL                    │
│ ┌───────────────────┐ ┌───────────────────┐            │
│ │   ID Column       │ │   Text Column     │            │
│ │ [ 1, 2, 3, 4 ]    │ │ [ &str, &str ]    │            │
│ └───────────────────┘ └───────────────────┘            │
└────────────────────────────────────────────────────────┘
```

The Arrow `StringArray` data buffers store string values as offset slices pointing directly to the memory-mapped read-only pages. By employing Rust's lifetime tracker (`'a`), the compiler statically ensures that no Arrow array or record batch can access string references after the memory map is unmapped or dropped.

---

## 2. O(1) Snapping Chunk Partitioner (Lock-Free)

To feed the multicore CPU CPU-bound parsing threads efficiently, the input must be partitioned into chunks. Standard partitioners parse the entire file sequentially first to count newline (`\n`) characters, introducing a massive serial bottleneck.

`parq` solves this by calculating fixed window partitions (default 64 MiB) and using a lock-free, O(1) boundary snapping algorithm:

```
[─── 64 MiB raw window ───][─── 64 MiB raw window ───][─── 64 MiB ───]
                          ▲                          ▲
                 Search forward             Search forward
                 for next '\n'              for next '\n'
                 (snapped boundary)         (snapped boundary)
```

1. **Split**: The mapped byte array is sliced into exact 64 MiB windows.
2. **Scan**: Each thread takes a slice and scans forward from the end of its window until it hits a newline byte (`b'\n'`).
3. **Bound**: The snapped index becomes the definitive ending boundary for that thread's chunk, and the starting boundary for the next chunk.

Because the scan window is extremely short (less than the length of one JSON line, usually a few hundred bytes), this partition step is O(1) and executes in microseconds, resulting in zero thread contention.

---

## 3. Bounded Backpressure Pipeline (Constant RAM)

When processing huge datasets (e.g. 100 GB files), parsing speed (parallel, multithreaded CPU work) will outrun writing speed (I/O-bound disk writes or network uploads). Without flow control, the parsing pool would continue generating `RecordBatch` allocations in RAM, causing memory exhaustion.

`parq` implements a bounded pipeline using a `crossbeam-channel` with a fixed depth (default 8):

```
 Rayon Workers (CPU-bound)       Bounded Channel (Depth=8)      Writer Thread (I/O-bound)
┌───────────┐ ┌───────────┐            ┌───────────┐                 ┌─────────────┐
│  Chunk 1  │ │  Chunk 2  │ ──Batch──► │  Batch 1  │ ─────Batch────► │ ArrowWriter │
└───────────┘ └───────────┘            ├───────────┤                 └─────────────┘
                                       │  Batch 2  │
                                       └───────────┘
                                             ▲
                                      Channel Full:
                                Rayon workers block/pause
```

* **Producer**: Rayon workers convert NDJSON chunks to Arrow `RecordBatch`es and send them over the channel.
* **Consumer**: A dedicated writing thread pulls `RecordBatch`es and streams them into the Parquet writer.
* **Backpressure**: When the channel contains 8 batches, the next worker trying to `send()` is blocked. The CPU threads pause parsing until the writer flushes data to disk, keeping RAM footprint entirely constant (~350 MB) regardless of input size.
