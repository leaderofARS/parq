# parq workspace Makefile
# Mirrors the structure used by pola-rs/polars

.PHONY: build release check test bench fmt lint clean doc

# ── Build targets ─────────────────────────────────────────────────────────────

build:
	cargo build --workspace

release:
	cargo build --workspace --release

release-simd:
	cargo build --workspace --release --features parq/simd

# ── Quality targets ───────────────────────────────────────────────────────────

check:
	cargo check --workspace

test:
	cargo test --workspace --lib

bench:
	cargo bench -p parq-core

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings

# ── Documentation ─────────────────────────────────────────────────────────────

doc:
	cargo doc --workspace --no-deps --open

# ── Data helpers ──────────────────────────────────────────────────────────────

generate-1m:
	python scripts/generate_data.py --rows 1_000_000 --output data_1m.jsonl

generate-10g:
	python scripts/generate_data.py --rows 5_000_000 --output data_10g.jsonl

benchmark-vs-python: data_1m.jsonl
	@echo "=== parq ==="
	./target/release/parq -i data_1m.jsonl -o out_rust.parquet -q
	@echo "=== Python (pandas + polars) ==="
	python scripts/python_baseline.py --input data_1m.jsonl --output out_py.parquet

# ── Cleanup ───────────────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -f *.jsonl *.parquet
