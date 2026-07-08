//! `parq` — CLI binary.  A thin wrapper over `parq_core::run_pipeline`.

use clap::Parser;
use parq_core::{run_pipeline, PipelineConfig};
use parq_io::parse_compression;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name    = "parq",
    version = "0.2.0",
    about   = "Zero-copy, multi-threaded NDJSON → Parquet converter",
    long_about = "
SYNOPSIS
    parq --input <FILE> --output <FILE> [OPTIONS]

DESCRIPTION
    Converts newline-delimited JSON to compressed Parquet.
    Uses a memory-mapped reader, fixed 64 MiB parallel chunks,
    and a bounded channel for constant-RAM backpressure.

EXAMPLES
    parq -i data.jsonl -o data.parquet
    parq -i data.jsonl -o data.parquet --schema schema.json -c zstd -t 32
    parq -i dirty.jsonl -o clean.parquet --ignore-errors --flatten
    parq -i data.jsonl --infer-schema-only
"
)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    input: String,

    #[arg(short, long, value_name = "FILE", required_unless_present = "infer_schema_only")]
    output: Option<String>,

    #[arg(short, long, default_value = "snappy", value_name = "CODEC")]
    compression: String,

    #[arg(short, long, default_value = "0", value_name = "N")]
    threads: usize,

    #[arg(long, default_value = "65536", value_name = "N")]
    batch_size: usize,

    #[arg(long, default_value = "8", value_name = "N")]
    channel_depth: usize,

    /// Lenient mode — skip malformed records, log to stderr
    #[arg(long)]
    ignore_errors: bool,

    /// Flatten nested objects: {"user":{"id":1}} → user_id: 1
    #[arg(long)]
    flatten: bool,

    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    #[arg(long, value_name = "FILE")]
    schema: Option<String>,

    #[arg(long, value_name = "BYTES")]
    chunk_size: Option<usize>,

    /// Print inferred schema as JSON and exit
    #[arg(long)]
    infer_schema_only: bool,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long)]
    quiet: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if !cli.quiet {
        let level = match cli.verbose { 0 => "warn", 1 => "info", 2 => "debug", _ => "trace" };
        fmt().with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level))
        ).with_target(false).compact().init();
    }

    if cli.infer_schema_only {
        use memmap2::MmapOptions;
        use std::fs::File;
        let file = File::open(&cli.input)?;
        // SAFETY: read-only handle, file not mutated during this call.
        let mmap   = unsafe { MmapOptions::new().map(&file)? };
        let schema = parq_schema::infer_schema(&mmap, 10_000)?;
        println!("{}", parq_schema::schema_to_json_string(&schema)?);
        return Ok(());
    }

    let metrics = run_pipeline(PipelineConfig {
        input_path:    cli.input,
        output_path:   cli.output.expect("--output required"),
        batch_size:    cli.batch_size,
        compression:   parse_compression(&cli.compression)?,
        num_threads:   cli.threads,
        ignore_errors: cli.ignore_errors,
        flatten:       cli.flatten,
        limit:         cli.limit,
        schema_path:   cli.schema,
        channel_depth: cli.channel_depth,
        chunk_size:    cli.chunk_size,
    })?;

    metrics.print_report();
    Ok(())
}
