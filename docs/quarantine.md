# Dead-Letter Quarantine

Real-world datasets are rarely clean. Large-scale ingestion pipelines often fail mid-way through a multi-hour batch job because of a few corrupt bytes, malformed JSON structures, or stray brackets.

`parq` solves this problem by providing a **Dead-Letter Quarantine** mechanism. When lenient mode is enabled, `parq` skips malformed lines, converts the rest of the clean records to Parquet, and writes the corrupt lines to a quarantined file for offline analysis.

---

## 1. Enabling the Quarantine

To configure the quarantine, use the `--dead-letter-path` argument along with the `--ignore-errors` flag:

```bash
parq -i dirty_data.jsonl \
     -o output.parquet \
     --ignore-errors \
     --dead-letter-path bad_records.jsonl
```

If using the Python bindings:
```python
import parq

metrics = parq.convert(
    input_path="dirty_data.jsonl",
    output_path="output.parquet",
    ignore_errors=True,
    dead_letter_path="bad_records.jsonl"
)
```

---

## 2. Dynamic Architecture (Lock-Free Appending)

Because `parq` splits parsing across multiple parallel worker threads, writing errors sequentially to a single file would create a major synchronization bottleneck.

To maintain maximum throughput, `parq` uses a dedicated background channel and writer thread:

1. **Parser Threads**: Rayon worker threads attempt to parse JSON chunks. If they encounter a malformed line (and `ignore_errors` is active), they send the raw bytes of that line over an unbounded channel to a background receiver.
2. **Quarantine Thread**: A dedicated thread consumes bytes from the channel and appends them sequentially to the dead-letter file.
3. **Execution**: This design keeps the parallel parsing threads completely non-blocking, ensuring zero contention.

---

## 3. Inspecting Quarantined Records

After the ingestion job completes, you can review the skipped lines. The quarantine output contains the exact raw bytes that failed to parse:

```bash
cat bad_records.jsonl
# Output:
# {"id": 12498, "text": "missing closing bracket"...
# THIS IS NOT JSON AT ALL
# {"id": 12502, "text": "stray comma",,}
```

Data engineers can fix the quarantined file and ingest it separately, ensuring **zero data loss** without crashing the main pipeline.
