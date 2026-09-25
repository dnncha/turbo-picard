//! Synthetic external-sort-core benchmark, not a genomics command benchmark.
//!
//! Build the same harness against both source revisions with:
//! rustc --edition=2024 -C opt-level=3 -A dead_code tools/bench_external_sort.rs
//! rustc --edition=2024 --test -A dead_code tools/bench_external_sort.rs
//! Arguments: NEW_OUTPUT_DIR SHAPE RECORDS RUN_RECORD_LIMIT FAN_IN.
//! SHAPE is random, ordered, reversed, or equal. Outputs are never overwritten.

#[path = "../crates/turbo-picard-core/src/external_sort.rs"]
mod external_sort;
#[path = "../crates/turbo-picard-core/src/merge_tree.rs"]
mod merge_tree;
#[path = "../crates/turbo-picard-core/src/temp_runs.rs"]
mod temp_runs;

use external_sort::{ExternalSortConfig, ExternalSorter};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: bench_external_sort NEW_OUTPUT_DIR SHAPE RECORDS RUN_LIMIT FAN_IN".into(),
        );
    }
    let output_dir = PathBuf::from(&args[0]);
    let shape = args[1].as_str();
    if !["random", "ordered", "reversed", "equal"].contains(&shape) {
        return Err("unknown shape".into());
    }
    let records: usize = args[2].parse()?;
    let limit: usize = args[3].parse()?;
    let fan_in: usize = args[4].parse()?;
    if records == 0 || limit == 0 || fan_in < 2 {
        return Err("records and run limit must be positive; fan-in must be at least two".into());
    }
    fs::create_dir(&output_dir)?;
    let scratch = output_dir.join("scratch");
    let mut config = ExternalSortConfig::new(&scratch);
    config.max_records_in_ram = limit;
    config.max_bytes_in_ram = 256 * 1024 * 1024;
    config.merge_fan_in = fan_in;
    let mut sorter = ExternalSorter::new(config)?;
    let output_path = output_dir.join("sorted.bin");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)?;
    let mut output = BufWriter::new(file);
    let mut state = 1_u64;
    let start = Instant::now();
    for ordinal in 0..records {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = match shape {
            "ordered" => ordinal as u64,
            "reversed" => (records - ordinal) as u64,
            "equal" => 0,
            _ => state % 65536,
        };
        let mut key = vec![0_u8; 16];
        key[8..].copy_from_slice(&value.to_be_bytes());
        let mut payload = vec![0_u8; 64];
        payload[..8].copy_from_slice(&(ordinal as u64).to_le_bytes());
        payload[8..16].copy_from_slice(&state.to_le_bytes());
        sorter.push(key, payload)?;
    }
    let mut previous: Option<([u8; 16], u64)> = None;
    let mut emitted = 0_usize;
    let metrics = sorter.finish_into(|item| {
        let key: [u8; 16] = item
            .key
            .as_slice()
            .try_into()
            .map_err(|_| "invalid key size")?;
        if let Some((old_key, old_ordinal)) = previous {
            if (old_key, old_ordinal) >= (key, item.ordinal) {
                return Err("output is not in strict key/ordinal order".to_string());
            }
        }
        previous = Some((key, item.ordinal));
        output
            .write_all(&item.key)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&item.ordinal.to_le_bytes())
            .map_err(|error| error.to_string())?;
        output
            .write_all(&item.payload)
            .map_err(|error| error.to_string())?;
        emitted += 1;
        Ok(())
    })?;
    output.flush()?;
    let seconds = start.elapsed().as_secs_f64();
    if emitted != records || fs::read_dir(&scratch)?.next().is_some() {
        return Err("record-count or scratch-cleanup verification failed".into());
    }
    let output_bytes = fs::metadata(output_path)?.len();
    println!(
        "{{\"scope\":\"synthetic_external_sort_core\",\"shape\":\"{shape}\",\"records\":{records},\"run_limit\":{limit},\"fan_in\":{fan_in},\"seconds\":{seconds:.9},\"spills\":{},\"run_count\":{},\"scratch_bytes_written\":{},\"max_resident_records\":{},\"max_estimated_bytes\":{},\"output_bytes\":{output_bytes}}}",
        metrics.spills,
        metrics.run_count,
        metrics.bytes_written,
        metrics.max_resident_records,
        metrics.max_estimated_bytes
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("external-sort benchmark: {error}");
        std::process::exit(1);
    }
}
