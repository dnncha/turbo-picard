# Jeanluc MarkDuplicates Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first testable Jeanluc foundation: Rust workspace, Picard-shaped CLI parsing, explicit unsupported behavior, correctness-comparison tooling, and benchmark scaffolding for `MarkDuplicates`.

**Architecture:** The CLI crate owns process behavior and delegates normalized command configuration into focused core crates. Correctness is proven with semantic tests and fixture tools before the native duplicate-marking engine expands beyond the coordinate-sorted BAM fast path. Benchmarks live beside correctness tooling so runtime claims are reproducible.

**Tech Stack:** Rust 1.95, Cargo workspace, `clap`-free custom Picard argument normalizer, `assert_cmd`, `predicates`, `tempfile`, shell-driven smoke commands, future `rust-htslib` integration.

---

## File Structure

- `Cargo.toml`: workspace root.
- `crates/jeanluc-cli/Cargo.toml`: binary crate manifest.
- `crates/jeanluc-cli/src/main.rs`: process entrypoint.
- `crates/jeanluc-cli/tests/cli.rs`: end-to-end CLI tests.
- `crates/jeanluc-core/Cargo.toml`: shared crate manifest.
- `crates/jeanluc-core/src/lib.rs`: core module exports.
- `crates/jeanluc-core/src/picard_args.rs`: Picard `KEY=VALUE` and long-option normalization.
- `crates/jeanluc-core/src/markdup_config.rs`: `MarkDuplicates` configuration validation.
- `crates/jeanluc-core/tests/picard_args.rs`: parser tests.
- `crates/jeanluc-core/tests/markdup_config.rs`: config tests.
- `tools/compare_markduplicates.py`: semantic comparison harness for Picard vs Jeanluc outputs.
- `tools/bench_markduplicates.py`: repeatable benchmark runner that captures wall time, RSS, command metadata, and output paths.
- `fixtures/README.md`: fixture policy and commands for generating or adding tiny compatibility fixtures.

## Task 1: Scaffold Workspace With A Failing CLI Test

**Files:**
- Create: `Cargo.toml`
- Create: `crates/jeanluc-cli/Cargo.toml`
- Create: `crates/jeanluc-cli/src/main.rs`
- Create: `crates/jeanluc-cli/tests/cli.rs`

- [ ] **Step 1: Write the failing CLI test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn unsupported_command_fails_clearly() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.arg("SortSam")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported Picard command: SortSam"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p jeanluc-cli unsupported_command_fails_clearly`

Expected: FAIL because the workspace or binary does not exist yet.

- [ ] **Step 3: Add minimal workspace and CLI implementation**

```toml
[workspace]
members = [
  "crates/jeanluc-cli",
  "crates/jeanluc-core",
]
resolver = "3"
```

```rust
fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("MarkDuplicates") => {
            eprintln!("MarkDuplicates is recognized but not implemented yet");
            std::process::exit(2);
        }
        Some(command) => {
            eprintln!("unsupported Picard command: {command}");
            std::process::exit(2);
        }
        None => {
            eprintln!("usage: jeanluc <PicardCommand> [KEY=VALUE ...]");
            std::process::exit(2);
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p jeanluc-cli unsupported_command_fails_clearly`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/jeanluc-cli
git commit -m "feat: scaffold jeanluc cli"
```

## Task 2: Normalize Picard Arguments

**Files:**
- Create: `crates/jeanluc-core/Cargo.toml`
- Create: `crates/jeanluc-core/src/lib.rs`
- Create: `crates/jeanluc-core/src/picard_args.rs`
- Create: `crates/jeanluc-core/tests/picard_args.rs`

- [ ] **Step 1: Write parser tests**

```rust
use jeanluc_core::picard_args::{normalize_picard_args, PicardArgError};

#[test]
fn normalizes_key_value_arguments() {
    let args = vec!["I=in.bam".to_string(), "O=out.bam".to_string(), "M=metrics.txt".to_string()];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(parsed.get("METRICS_FILE").unwrap(), &vec!["metrics.txt".to_string()]);
}

#[test]
fn normalizes_long_options() {
    let args = vec![
        "--INPUT".to_string(),
        "in.bam".to_string(),
        "--OUTPUT=out.bam".to_string(),
        "--METRICS_FILE".to_string(),
        "metrics.txt".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(parsed.get("METRICS_FILE").unwrap(), &vec!["metrics.txt".to_string()]);
}

#[test]
fn rejects_positional_arguments() {
    let args = vec!["in.bam".to_string()];
    let err = normalize_picard_args(&args).unwrap_err();
    assert_eq!(err, PicardArgError::UnexpectedPositional("in.bam".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jeanluc-core --test picard_args`

Expected: FAIL because `jeanluc-core` parser modules are missing.

- [ ] **Step 3: Implement the parser**

Implement `normalize_picard_args(args: &[String]) -> Result<BTreeMap<String, Vec<String>>, PicardArgError>` with aliases `I -> INPUT`, `O -> OUTPUT`, and `M -> METRICS_FILE`. Treat `KEY=VALUE`, `--KEY VALUE`, and `--KEY=VALUE` as equivalent.

- [ ] **Step 4: Run parser tests**

Run: `cargo test -p jeanluc-core --test picard_args`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jeanluc-core
git commit -m "feat: normalize picard arguments"
```

## Task 3: Validate MarkDuplicates Configuration

**Files:**
- Create: `crates/jeanluc-core/src/markdup_config.rs`
- Create: `crates/jeanluc-core/tests/markdup_config.rs`
- Modify: `crates/jeanluc-core/src/lib.rs`

- [ ] **Step 1: Write config tests**

```rust
use jeanluc_core::markdup_config::{MarkDuplicatesConfig, MarkDuplicatesConfigError};
use jeanluc_core::picard_args::normalize_picard_args;

#[test]
fn accepts_minimal_required_picard_arguments() {
    let args = vec!["I=in.bam".to_string(), "O=out.bam".to_string(), "M=metrics.txt".to_string()];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");
    assert_eq!(config.input, "in.bam");
    assert_eq!(config.output, "out.bam");
    assert_eq!(config.metrics_file, "metrics.txt");
    assert!(!config.remove_duplicates);
}

#[test]
fn parses_remove_duplicates_boolean() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "REMOVE_DUPLICATES=true".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");
    assert!(config.remove_duplicates);
}

#[test]
fn rejects_missing_metrics_file() {
    let args = vec!["I=in.bam".to_string(), "O=out.bam".to_string()];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();
    assert_eq!(err, MarkDuplicatesConfigError::MissingRequired("METRICS_FILE"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jeanluc-core --test markdup_config`

Expected: FAIL because `MarkDuplicatesConfig` does not exist.

- [ ] **Step 3: Implement config validation**

Create a small config type with `input`, `output`, `metrics_file`, and `remove_duplicates`. Reject missing required values, duplicate scalar values, invalid booleans, and unsupported keys.

- [ ] **Step 4: Run config tests**

Run: `cargo test -p jeanluc-core --test markdup_config`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jeanluc-core
git commit -m "feat: validate markduplicates config"
```

## Task 4: Wire CLI To Validated MarkDuplicates Config

**Files:**
- Modify: `crates/jeanluc-cli/Cargo.toml`
- Modify: `crates/jeanluc-cli/src/main.rs`
- Modify: `crates/jeanluc-cli/tests/cli.rs`

- [ ] **Step 1: Write CLI config tests**

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn markduplicates_requires_metrics_file() {
    let mut cmd = Command::cargo_bin("jeanluc").expect("binary exists");
    cmd.args(["MarkDuplicates", "I=in.bam", "O=out.bam"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing required MarkDuplicates argument: METRICS_FILE"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jeanluc-cli markduplicates_requires_metrics_file`

Expected: FAIL because CLI does not yet call config validation.

- [ ] **Step 3: Implement CLI validation dispatch**

Call `normalize_picard_args`, then `MarkDuplicatesConfig::try_from_args`, then exit with a clear "recognized but native engine is not implemented yet" error until the engine crate lands.

- [ ] **Step 4: Run CLI tests**

Run: `cargo test -p jeanluc-cli`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jeanluc-cli
git commit -m "feat: validate markduplicates cli"
```

## Task 5: Add Semantic Correctness Harness

**Files:**
- Create: `tools/compare_markduplicates.py`
- Create: `fixtures/README.md`

- [ ] **Step 1: Write a failing harness self-test command**

Run: `python3 tools/compare_markduplicates.py --help`

Expected: FAIL because the tool does not exist.

- [ ] **Step 2: Implement comparison harness**

The tool accepts `--picard-bam`, `--jeanluc-bam`, `--picard-metrics`, and `--jeanluc-metrics`. It compares SAM records semantically by query name, flag duplicate bit, reference id, position, mate reference id, mate position, CIGAR, and template length. It compares metrics files while ignoring comment lines and tolerating exact numeric equality first.

- [ ] **Step 3: Run harness help**

Run: `python3 tools/compare_markduplicates.py --help`

Expected: PASS and print usage.

- [ ] **Step 4: Commit**

```bash
git add tools/compare_markduplicates.py fixtures/README.md
git commit -m "test: add markduplicates semantic comparison harness"
```

## Task 6: Add Benchmark Harness

**Files:**
- Create: `tools/bench_markduplicates.py`

- [ ] **Step 1: Write a failing benchmark help command**

Run: `python3 tools/bench_markduplicates.py --help`

Expected: FAIL because the tool does not exist.

- [ ] **Step 2: Implement benchmark runner**

The runner accepts Picard command, Jeanluc command, input BAM, output directory, repeat count, and optional warmup. It captures wall-clock duration, max RSS from `/usr/bin/time -l` on macOS, exit code, stderr path, stdout path, and output artifact paths into JSONL.

- [ ] **Step 3: Run benchmark help**

Run: `python3 tools/bench_markduplicates.py --help`

Expected: PASS and print usage.

- [ ] **Step 4: Commit**

```bash
git add tools/bench_markduplicates.py
git commit -m "bench: add markduplicates benchmark harness"
```

## Task 7: Native MarkDuplicates Engine Milestone

**Files:**
- Create: `crates/jeanluc-markdup/Cargo.toml`
- Create: `crates/jeanluc-markdup/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `crates/jeanluc-cli/Cargo.toml`
- Modify: `crates/jeanluc-cli/src/main.rs`
- Add tests under: `crates/jeanluc-markdup/tests/`

- [ ] **Step 1: Add a tiny BAM fixture or fixture-generation script**

Use a deterministic fixture with one duplicate pair and one unique pair. The expected behavior is exactly one pair marked duplicate and Picard-compatible metrics for the fixture.

- [ ] **Step 2: Run the fixture test to verify it fails**

Run: `cargo test -p jeanluc-markdup`

Expected: FAIL because the native engine does not exist.

- [ ] **Step 3: Implement the smallest coordinate-sorted BAM path**

Use `rust-htslib` to read BAM, group candidate duplicates, mark duplicate flags, write BAM, and emit metrics. Implement only the fixture-proven behavior first.

- [ ] **Step 4: Run correctness tests and comparison harness**

Run: `cargo test -p jeanluc-markdup`

Expected: PASS for the tiny fixture.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/jeanluc-cli crates/jeanluc-markdup fixtures
git commit -m "feat: add initial markduplicates engine"
```

## Task 8: Expand Correctness Matrix Before Performance Claims

**Files:**
- Add fixtures under: `fixtures/markduplicates/`
- Add tests under: `crates/jeanluc-markdup/tests/`

- [ ] **Step 1: Add failing tests for single-end reads, paired reads, secondary/supplementary reads, duplicate scoring, and remove-duplicates mode**
- [ ] **Step 2: Run the focused tests and confirm each fails for the expected unsupported behavior**
- [ ] **Step 3: Implement one behavior at a time**
- [ ] **Step 4: Run all correctness tests and semantic comparison harness**
- [ ] **Step 5: Commit after each behavior**

## Task 9: Benchmark Against Picard On Representative Inputs

**Files:**
- Modify: `tools/bench_markduplicates.py`
- Add: `benchmarks/README.md`

- [ ] **Step 1: Run benchmarks on tiny fixtures to prove the harness**
- [ ] **Step 2: Run benchmarks on at least one realistic WGS or WES coordinate-sorted BAM**
- [ ] **Step 3: Record machine details, commands, Picard version, Jeanluc commit, wall time, RSS, and correctness comparison result**
- [ ] **Step 4: Do not claim speedup unless the semantic comparison passes**
- [ ] **Step 5: Commit benchmark documentation**
