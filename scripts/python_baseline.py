#!/usr/bin/env python3
"""
Python baseline benchmark: JSON → Parquet using pandas + pyarrow.

This script deliberately mirrors what the Rust pipeline does so that
the comparison is apples-to-apples:
  1. Read a newline-delimited JSON file entirely into memory.
  2. Parse all records.
  3. Write a compressed Parquet file.

Usage:
    pip install pandas pyarrow polars psutil
    python scripts/python_baseline.py --input data.jsonl --output data_py.parquet

The script prints a metrics table identical in shape to the Rust CLI output.
"""

import argparse
import gc
import json
import os
import sys
import time
from pathlib import Path

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


def memory_mb() -> float:
    if not HAS_PSUTIL:
        return 0.0
    proc = psutil.Process(os.getpid())
    return proc.memory_info().rss / 1_048_576


def run_pandas(input_path: str, output_path: str, compression: str) -> dict:
    import pandas as pd

    gc.collect()
    mem_before = memory_mb()
    t0 = time.perf_counter()

    df = pd.read_json(input_path, lines=True)
    t_parse = time.perf_counter()

    df.to_parquet(output_path, compression=compression, index=False)
    t_write = time.perf_counter()

    mem_after = memory_mb()

    return {
        "engine": "pandas",
        "rows": len(df),
        "parse_ms": (t_parse - t0) * 1000,
        "write_ms": (t_write - t_parse) * 1000,
        "total_ms": (t_write - t0) * 1000,
        "peak_mem_delta_mb": max(0, mem_after - mem_before),
    }


def run_polars(input_path: str, output_path: str, compression: str) -> dict:
    import polars as pl

    gc.collect()
    mem_before = memory_mb()
    t0 = time.perf_counter()

    df = pl.read_ndjson(input_path)
    t_parse = time.perf_counter()

    df.write_parquet(output_path, compression=compression)
    t_write = time.perf_counter()

    mem_after = memory_mb()

    return {
        "engine": "polars",
        "rows": len(df),
        "parse_ms": (t_parse - t0) * 1000,
        "write_ms": (t_write - t_parse) * 1000,
        "total_ms": (t_write - t0) * 1000,
        "peak_mem_delta_mb": max(0, mem_after - mem_before),
    }


def print_result(r: dict, input_bytes: int, output_bytes: int) -> None:
    throughput = (input_bytes / 1_048_576) / (r["total_ms"] / 1000)
    rows_per_sec = r["rows"] / (r["total_ms"] / 1000)
    compression = input_bytes / output_bytes if output_bytes else 0

    sep = "─" * 52
    print(f"\n┌{sep}┐")
    print(f"│{'  Python Baseline Metrics (' + r['engine'] + ')':^52}│")
    print(f"├{sep}┤")
    rows = [
        ("Input size",     f"{input_bytes / 1_048_576:.2f} MiB"),
        ("Output size",    f"{output_bytes / 1_048_576:.2f} MiB"),
        ("Compression",    f"{compression:.2f}×"),
        ("Rows processed", f"{r['rows']:,}"),
        ("  ↳ Parse",      f"{r['parse_ms']:.0f} ms"),
        ("  ↳ Write",      f"{r['write_ms']:.0f} ms"),
        ("Total time",     f"{r['total_ms']:.0f} ms"),
        ("Throughput",     f"{throughput:.1f} MiB/s"),
        ("Row rate",       f"{rows_per_sec:,.0f} rows/s"),
        ("Mem delta",      f"{r['peak_mem_delta_mb']:.0f} MB"),
    ]
    for label, value in rows:
        print(f"│  {label:<22}  {value:>24}  │")
    print(f"└{sep}┘")


def main():
    p = argparse.ArgumentParser(description="Python JSON → Parquet baseline benchmark")
    p.add_argument("--input", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--engine", choices=["pandas", "polars", "both"], default="both")
    p.add_argument("--compression", default="snappy",
                   choices=["snappy", "gzip", "brotli", "zstd", "lz4", "none"])
    args = p.parse_args()

    input_bytes = os.path.getsize(args.input)

    if args.engine in ("pandas", "both"):
        out = args.output.replace(".parquet", "_pandas.parquet")
        result = run_pandas(args.input, out, args.compression)
        out_bytes = os.path.getsize(out)
        print_result(result, input_bytes, out_bytes)

    if args.engine in ("polars", "both"):
        out = args.output.replace(".parquet", "_polars.parquet")
        result = run_polars(args.input, out, args.compression)
        out_bytes = os.path.getsize(out)
        print_result(result, input_bytes, out_bytes)


if __name__ == "__main__":
    main()
