# Architecture

`parq` achieves ultra-high throughput by combining modern systems techniques:

---

## 1. Memory-Mapped Zero-Copy Parsing

Instead of reading file chunks sequentially using `read()` system calls (which pollute CPU caches and trigger copy operations), `parq` memory-maps the input file directly into the virtual memory space using `memmap2`.

Rust's lifetime checker proves that no data slice outlives the memory map:
```rust
// The lifetime 'a binds every subslice to the memory map itself.
pub fn fixed_chunks(data: &[u8], chunk_size: Option<usize>) -> Vec<&[u8]>
```

String values are deserialized as borrowed `&str` references pointing directly inside the memory-mapped read-only pages. No memory reallocation is triggered for strings.

---

## 2. Parallel Boundary Snapping (Zero Contention)

To divide work cleanly among multiple CPU cores without reading the file twice, `parq` splits the mapped buffer into logical 64 MiB slices:

```
[─── 64 MiB raw window ───][─── 64 MiB raw window ───][─── 64 MiB ───]
        │ snap forward to \n        │ snap forward to \n       │ snap…
   zero thread contention     O(1) scan per boundary     no coord pass
```

Each thread:
1. Takes a fixed 64 MiB window.
2. Scans forward a few bytes from the end of its partition until it hits `\n` to establish its definitive boundary.
3. The next thread starts exactly one byte after that.

This guarantees zero synchronization contention during partitioning and matches OS memory-page layout boundaries perfectly.

---

## 3. Bounded Backpressured Writing

Ingestion speed (CPU-bound JSON parsing) can easily outrun disk flushing (I/O-bound Parquet writing). To prevent parsed record batches from accumulating in RAM, `parq` implements a bounded pipeline:

```
rayon pool ──[RecordBatch]──► bounded channel (depth=8) ──► writer thread
                                      │
                   channel full: rayon worker blocks → constant RAM
```

If the writer thread hits disk latency, the `crossbeam` channel fills up, causing Rayon worker threads to pause parsing until space is cleared. Memory footprint is strictly capped.
