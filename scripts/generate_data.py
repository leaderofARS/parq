#!/usr/bin/env python3
"""
Generate a synthetic newline-delimited JSON dataset for benchmarking.

Usage:
    python scripts/generate_data.py --rows 5000000 --output data.jsonl
    # → ~10 GiB file with 5M realistic LLM training data records
"""

import argparse
import json
import random
import string
import sys
import time
from pathlib import Path

CATEGORIES = ["science", "technology", "finance", "health", "sports", "politics", "culture"]
TAGS = ["ai", "ml", "llm", "nlp", "cv", "rl", "data", "cloud", "gpu", "edge", "rust", "python"]
LOREM_WORDS = (
    "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor "
    "incididunt ut labore et dolore magna aliqua ut enim ad minim veniam quis nostrud "
    "exercitation ullamco laboris nisi aliquip ex ea commodo consequat duis aute irure "
    "dolor in reprehenderit voluptate velit esse cillum dolore eu fugiat nulla pariatur "
    "excepteur sint occaecat cupidatat non proident culpa qui officia deserunt mollit "
    "anim est laborum training data record synthetic generation benchmark pipeline"
).split()


def random_text(min_words: int = 20, max_words: int = 80) -> str:
    n = random.randint(min_words, max_words)
    return " ".join(random.choices(LOREM_WORDS, k=n))


def generate_record(i: int) -> dict:
    return {
        "id": i,
        "user_id": f"usr_{i:010d}",
        "session_id": f"sess_{random.randint(1, 1_000_000):08x}",
        "timestamp": f"2024-{random.randint(1,12):02d}-{random.randint(1,28):02d}T"
                     f"{random.randint(0,23):02d}:{random.randint(0,59):02d}:{random.randint(0,59):02d}Z",
        "category": random.choice(CATEGORIES),
        "tags": random.sample(TAGS, k=random.randint(1, 4)),
        "score": round(random.gauss(0.5, 0.2), 6),
        "rank": random.randint(1, 10_000),
        "active": random.random() > 0.15,
        "metadata": {
            "source": random.choice(["web", "api", "mobile", "batch"]),
            "version": f"{random.randint(1,3)}.{random.randint(0,9)}.{random.randint(0,9)}",
        },
        "text": random_text(),
        "embedding_norm": round(random.uniform(0.8, 1.2), 8),
        "token_count": random.randint(15, 512),
    }


def main():
    parser = argparse.ArgumentParser(description="Generate synthetic NDJSON benchmark data")
    parser.add_argument("--rows", type=int, default=1_000_000, help="Number of JSON records")
    parser.add_argument("--output", default="data.jsonl", help="Output file path")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    args = parser.parse_args()

    random.seed(args.seed)
    out = Path(args.output)

    print(f"Generating {args.rows:,} records → {out} …")
    t0 = time.perf_counter()

    with open(out, "w", encoding="utf-8", buffering=1 << 22) as f:
        for i in range(args.rows):
            record = generate_record(i)
            f.write(json.dumps(record, separators=(",", ":")) + "\n")
            if (i + 1) % 100_000 == 0:
                elapsed = time.perf_counter() - t0
                rate = (i + 1) / elapsed
                pct = (i + 1) / args.rows * 100
                print(f"  {pct:5.1f}%  {i+1:>10,} rows  {rate:>10,.0f} rows/s", file=sys.stderr)

    size_mb = out.stat().st_size / 1_048_576
    elapsed = time.perf_counter() - t0
    print(f"\nDone! {args.rows:,} rows in {elapsed:.1f}s → {size_mb:.1f} MiB ({out})")


if __name__ == "__main__":
    main()
