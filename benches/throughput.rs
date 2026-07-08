//! Criterion benchmarks: chunk splitting, schema inference, parse throughput.
//!
//! Run with:  cargo bench
//! Reports:   target/criterion/report/index.html

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use parq::{chunk, parser, schema};
use std::sync::Arc;

fn synthetic_ndjson(n_rows: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(n_rows * 128);
    for i in 0..n_rows {
        let line = format!(
            r#"{{"id":{i},"user_id":"usr_{i}","score":{score:.6},"active":{active},"text":"synthetic record {i} with realistic text content for sizing"}}"#,
            score = (i as f64 * 0.001_337) % 1.0,
            active = i % 3 != 0,
        );
        buf.extend_from_slice(line.as_bytes());
        buf.push(b'\n');
    }
    buf
}

fn bench_fixed_chunks(c: &mut Criterion) {
    let mut g = c.benchmark_group("fixed_chunks");
    let data = synthetic_ndjson(500_000);

    for chunk_mib in [4usize, 16, 64] {
        let chunk_bytes = chunk_mib * 1024 * 1024;
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(
            BenchmarkId::new("chunk_mib", chunk_mib),
            &chunk_bytes,
            |b, &cs| b.iter(|| chunk::fixed_chunks(&data, Some(cs))),
        );
    }
    g.finish();
}

fn bench_schema_infer(c: &mut Criterion) {
    let mut g = c.benchmark_group("schema_infer");
    let data = synthetic_ndjson(20_000);

    for sample in [100usize, 1_000, 10_000] {
        g.bench_with_input(
            BenchmarkId::new("sample_rows", sample),
            &sample,
            |b, &s| b.iter(|| schema::infer_schema(&data, s).unwrap()),
        );
    }
    g.finish();
}

fn bench_parse_chunk(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_chunk");

    for n_rows in [10_000usize, 100_000, 500_000] {
        let data = synthetic_ndjson(n_rows);
        let schema = Arc::new(schema::infer_schema(&data, 100).unwrap());

        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(
            BenchmarkId::new("rows", n_rows),
            &data,
            |b, data| {
                b.iter(|| {
                    parser::parse_chunk(
                        data,
                        Arc::clone(&schema),
                        65_536,
                        false, // strict mode
                        false, // no flatten
                        None,
                    )
                    .unwrap()
                })
            },
        );
    }
    g.finish();
}

fn bench_parse_with_flatten(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_flatten");
    // Nested data to stress the flatten path
    let data: Vec<u8> = (0..100_000)
        .flat_map(|i| {
            format!(
                r#"{{"id":{i},"user":{{"id":{i},"score":{:.4}}},"meta":{{"src":"web","v":2}}}}\n"#,
                (i as f64) * 0.001
            )
            .into_bytes()
        })
        .collect();

    let schema = Arc::new(schema::infer_schema(&data, 200).unwrap());

    g.throughput(Throughput::Bytes(data.len() as u64));
    g.bench_function("flatten=true", |b| {
        b.iter(|| {
            parser::parse_chunk(&data, Arc::clone(&schema), 65_536, false, true, None).unwrap()
        })
    });
    g.bench_function("flatten=false", |b| {
        b.iter(|| {
            parser::parse_chunk(&data, Arc::clone(&schema), 65_536, false, false, None).unwrap()
        })
    });
    g.finish();
}

criterion_group!(
    benches,
    bench_fixed_chunks,
    bench_schema_infer,
    bench_parse_chunk,
    bench_parse_with_flatten
);
criterion_main!(benches);
