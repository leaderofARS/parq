//! Criterion throughput benchmarks for `parq-core`.
//!
//! Run: `cargo bench -p parq-core`
//! HTML report: `target/criterion/report/index.html`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use parq_chunk::fixed_chunks;
use parq_parser::parse_chunk;
use parq_schema::infer_schema;
use std::sync::Arc;

fn synth(rows: usize) -> Vec<u8> {
    (0..rows)
        .flat_map(|i| {
            format!(
                r#"{{"id":{i},"user_id":"u_{i}","score":{:.6},"active":{}}}\n"#,
                (i as f64 * 0.001_337) % 1.0,
                i % 3 != 0,
            )
            .into_bytes()
        })
        .collect()
}

fn bench_fixed_chunks(c: &mut Criterion) {
    let mut g = c.benchmark_group("fixed_chunks");
    let data = synth(500_000);
    for mib in [4usize, 16, 64] {
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::new("chunk_mib", mib), &mib, |b, &mib| {
            b.iter(|| fixed_chunks(&data, Some(mib * 1024 * 1024)))
        });
    }
    g.finish();
}

fn bench_schema_infer(c: &mut Criterion) {
    let mut g = c.benchmark_group("schema_infer");
    let data = synth(20_000);
    for sample in [100usize, 1_000, 10_000] {
        g.bench_with_input(BenchmarkId::new("sample", sample), &sample, |b, &s| {
            b.iter(|| infer_schema(&data, s).unwrap())
        });
    }
    g.finish();
}

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_chunk");
    for rows in [10_000usize, 100_000, 500_000] {
        let data   = synth(rows);
        let schema = Arc::new(infer_schema(&data, 100).unwrap());
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::new("rows", rows), &data, |b, d| {
            b.iter(|| parse_chunk(d, Arc::clone(&schema), 65_536, false, false, None).unwrap())
        });
    }
    g.finish();
}

criterion_group!(benches, bench_fixed_chunks, bench_schema_infer, bench_parse);
criterion_main!(benches);
