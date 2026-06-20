use assert_cmd::Command;
use flate2::read::GzDecoder;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;

#[test]
fn collecthsmetrics_help_exposes_scaffold_surface() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args(["CollectHsMetrics", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Native bait/target accumulation is not implemented yet",
        ));
}

#[test]
fn list_commands_exposes_picard_reference_surface() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.arg("--list-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("CollectHsMetrics"))
        .stdout(predicate::str::contains("CollectSamErrorMetrics"))
        .stdout(predicate::str::contains("MarkDuplicates"));
}

#[test]
fn acceleration_status_reports_cpu_backend() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.arg("AccelerationStatus")
        .env("TURBO_PICARD_THREADS", "3")
        .env("TURBO_PICARD_ACCELERATOR", "cpu")
        .assert()
        .success()
        .stdout(predicate::str::contains("backend=cpu"))
        .stdout(predicate::str::contains("policy=cpu"))
        .stdout(predicate::str::contains("htslib_worker_threads=3"))
        .stdout(predicate::str::contains("gpu_acceleration=not-enabled"));
}

#[test]
fn acceleration_status_rejects_required_gpu_without_backend() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.arg("AccelerationStatus")
        .env("TURBO_PICARD_ACCELERATOR", "gpu-required")
        .assert()
        .failure()
        .stdout(predicate::str::contains("policy=gpu-required"))
        .stderr(predicate::str::contains(
            "this build has no production GPU backend",
        ));
}

#[test]
fn acceleration_status_supports_help_smoke_check() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args(["AccelerationStatus", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: picard AccelerationStatus"))
        .stdout(predicate::str::contains("gpu_acceleration"));
}

#[test]
fn doctor_reports_runtime_and_fallback_state() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);

    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.arg("doctor")
        .env(
            "TURBO_PICARD_FALLBACK_COMMAND",
            fallback.display().to_string(),
        )
        .env("TURBO_PICARD_THREADS", "2")
        .assert()
        .success()
        .stdout(predicate::str::contains("turbo_picard_version="))
        .stdout(predicate::str::contains("picard_reference_version=3.4.0"))
        .stdout(predicate::str::contains("backend=cpu"))
        .stdout(predicate::str::contains("htslib_worker_threads=2"))
        .stdout(predicate::str::contains(format!(
            "fallback_command={}",
            fallback.display()
        )));
}

#[test]
fn explain_reports_native_scope_and_declared_outputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);

    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args([
        "explain",
        "MarkDuplicates",
        "I=input.bam",
        "O=marked.bam",
        "M=metrics.txt",
    ])
    .env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("command=MarkDuplicates"))
    .stdout(predicate::str::contains("status=partial-native"))
    .stdout(predicate::str::contains(
        "execution_path=native-when-inside-documented-scope-otherwise-fallback",
    ))
    .stdout(predicate::str::contains(
        "declared_outputs=O=marked.bam,M=metrics.txt",
    ));
}

#[test]
fn explain_reports_fallback_only_reference_commands() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args(["explain", "EstimateLibraryComplexity", "O=metrics.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "command=EstimateLibraryComplexity",
        ))
        .stdout(predicate::str::contains("status=fallback-only"))
        .stdout(predicate::str::contains("fallback_command="))
        .stdout(predicate::str::contains("declared_outputs=O=metrics.txt"));
}

#[test]
fn markduplicates_requires_metrics_file() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args(["MarkDuplicates", "I=in.bam", "O=out.bam"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "missing required MarkDuplicates argument: METRICS_FILE",
        ));
}

#[test]
fn markduplicates_rejects_unsupported_option() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args([
        "MarkDuplicates",
        "I=in.bam",
        "O=out.bam",
        "M=metrics.txt",
        "TAGGING_POLICY=Invalid",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "unsupported MarkDuplicates argument: TAGGING_POLICY=Invalid",
    ));
}

#[test]
fn unsupported_command_delegates_to_configured_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 17);
    let log = tempdir.path().join("fallback.args");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args(["EstimateLibraryComplexity", "I=in.bam", "O=metrics.txt"])
    .assert()
    .code(17);

    let fallback_args = fs::read_to_string(log).expect("fallback log exists");
    assert_eq!(
        fallback_args,
        "EstimateLibraryComplexity\nI=in.bam\nO=metrics.txt\n"
    );
}

#[test]
fn jvm_style_leading_args_delegate_to_configured_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);
    let log = tempdir.path().join("fallback.args");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args(["-Xmx2g", "ValidateSamFile", "I=in.bam", "MODE=SUMMARY"])
    .assert()
    .success();

    let fallback_args = fs::read_to_string(log).expect("fallback log exists");
    assert_eq!(
        fallback_args,
        "-Xmx2g\nValidateSamFile\nI=in.bam\nMODE=SUMMARY\n"
    );
}

#[test]
fn native_io_failure_does_not_delegate_to_configured_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);
    let log = tempdir.path().join("fallback.args");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args([
        "MarkDuplicates",
        "I=/no/such/input.bam",
        &format!("O={}", output.display()),
        &format!("M={}", metrics.display()),
    ])
    .assert()
    .failure();

    assert!(!log.exists());
}

#[test]
fn markduplicates_delegates_unsupported_native_surface_to_configured_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);
    let log = tempdir.path().join("fallback.args");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args([
        "MarkDuplicates",
        "I=in.bam",
        "O=out.bam",
        "M=metrics.txt",
        "TAGGING_POLICY=Invalid",
    ])
    .assert()
    .success();

    let fallback_args = fs::read_to_string(log).expect("fallback log exists");
    assert_eq!(
        fallback_args,
        "MarkDuplicates\nI=in.bam\nO=out.bam\nM=metrics.txt\nTAGGING_POLICY=Invalid\n"
    );
}

#[test]
fn markduplicates_uses_native_engine_before_configured_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 99);
    let log = tempdir.path().join("fallback.args");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args([
        "MarkDuplicates",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        &format!("M={}", metrics.display()),
    ])
    .assert()
    .success();

    assert!(!log.exists());
    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("read-b\t1024\tchr1\t10"));
}

#[test]
fn markduplicates_marks_duplicate_sam_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-c\t0\tchr1\t50\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args([
        "MarkDuplicates",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        &format!("M={}", metrics.display()),
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("read-a\t0\tchr1\t10"));
    assert!(output_sam.contains("read-b\t1024\tchr1\t10"));
    assert!(output_sam.contains("read-c\t0\tchr1\t50"));

    let metrics_text = fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("UNPAIRED_READ_DUPLICATES"));
    assert!(metrics_text.contains("Unknown Library\t3\t0\t0\t0\t1\t0\t0\t0.333333\t\n"));
}

#[test]
fn sortsam_sorts_sam_by_coordinate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("coordinate.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SortSam",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@HD\tVN:1.6\tSO:coordinate"));
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn sortsam_sorts_sam_by_queryname() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("queryname.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args([
        "SortSam",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "SO=queryname",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@HD\tVN:1.6\tSO:queryname"));
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn sortsam_sam_text_uses_external_sorter_for_forced_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("coordinate.sam");
    let sort_tmp = tempdir.path().join("sort-tmp");
    fs::create_dir(&sort_tmp).expect("sort tmp exists");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "late\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
            "dup\t0\tchr1\t20\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "dup\t0\tchr1\t20\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
            "early\t0\tchr1\t10\t60\t4M\t*\t0\t0\tGGGG\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "SO=coordinate",
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
        ])
        .assert()
        .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(
        record_names(&output_sam),
        vec!["early", "dup", "dup", "late"]
    );
    assert_eq!(
        output_sam
            .lines()
            .filter(|line| !line.starts_with('@'))
            .map(|line| line.split('\t').nth(9).expect("sequence field"))
            .collect::<Vec<_>>(),
        vec!["GGGG", "AAAA", "TTTT", "CCCC"]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("sort tmp readable")
            .next()
            .is_none(),
        "SortSam SAM text external sort should clean temporary runs"
    );
}

#[test]
fn cleansam_sets_unmapped_mapq_to_zero_and_preserves_valid_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("cleaned.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t60\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CleanSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("cleaned SAM exists");
    assert!(output.contains("mapped\t0\tchr1\t10\t60\t4M"));
    assert!(output.contains("unmapped\t4\t*\t0\t0\t*"));
}

#[test]
fn cleansam_soft_clips_simple_trailing_reference_overhang() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("cleaned.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:10\n",
            "overhang\t0\tchr1\t8\t60\t5M\t*\t0\t0\tACGTA\tFFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CleanSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("cleaned SAM exists");
    assert!(output.contains("overhang\t0\tchr1\t8\t60\t3M2S"));
}

#[test]
fn cleansam_writes_requested_bam_sidecars() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("cleaned.bam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:10\n",
            "mapped\t0\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "overhang\t0\tchr1\t8\t60\t5M\t*\t0\t0\tACGTA\tFFFFF\n",
            "unmapped\t4\t*\t0\t60\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input SAM is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CleanSam",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "CREATE_MD5_FILE=true",
        "CREATE_INDEX=true",
        "COMPRESSION_LEVEL=5",
        "MAX_RECORDS_IN_RAM=500",
        &format!("TMP_DIR={}", tempdir.path().display()),
        "VERBOSITY=WARNING",
    ])
    .assert()
    .success();

    assert!(output.exists());
    assert!(tempdir.path().join("cleaned.bam.md5").exists());
    assert!(tempdir.path().join("cleaned.bai").exists());
}

#[test]
fn sortsam_writes_requested_bam_sidecars() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("coordinate.bam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SortSam",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
        "CREATE_MD5_FILE=true",
        "CREATE_INDEX=true",
        "COMPRESSION_LEVEL=5",
        "MAX_RECORDS_IN_RAM=500",
        &format!("TMP_DIR={}", tempdir.path().display()),
        "VERBOSITY=WARNING",
    ])
    .assert()
    .success();

    assert!(output.exists());
    assert!(tempdir.path().join("coordinate.bam.md5").exists());
    assert!(tempdir.path().join("coordinate.bai").exists());
}

#[test]
fn sortsam_bam_uses_bounded_temp_runs_for_coordinate_sort() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("queryname.bam");
    let output = tempdir.path().join("coordinate.sam");
    let sort_tmp = tempdir.path().join("sort-tmp");
    fs::create_dir(&sort_tmp).expect("sort tmp exists");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "a10\t0\tchr1\t10\t60\t4M\t*\t0\t0\tGGGG\tFFFF\n",
            "b20\t0\tchr1\t20\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "b20\t0\tchr1\t20\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
            "z05\t0\tchr1\t5\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=queryname",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output.display()),
            "SO=coordinate",
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "QUIET=true",
        ])
        .assert()
        .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(record_names(&output_sam), vec!["z05", "a10", "b20", "b20"]);
    assert_eq!(
        output_sam
            .lines()
            .filter(|line| !line.starts_with('@'))
            .map(|line| line.split('\t').nth(9).expect("sequence field"))
            .collect::<Vec<_>>(),
        vec!["CCCC", "GGGG", "AAAA", "TTTT"]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("sort tmp readable")
            .next()
            .is_none(),
        "SortSam BAM temp runs should be cleaned"
    );
}

#[test]
fn sortsam_streams_already_sorted_input() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("coordinate.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
            "read-c\t0\tchr1\t90\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SortSam",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@HD\tVN:1.6\tSO:coordinate"));
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn buildbamindex_writes_default_and_explicit_bai_outputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let explicit_bai = tempdir.path().join("explicit.bai");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SORT_ORDER=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args(["BuildBamIndex", &format!("I={}", input_bam.display())])
        .assert()
        .success();
    assert!(tempdir.path().join("input.bai").exists());

    Command::cargo_bin("turbo-picard")
        .expect("binary exists")
        .args([
            "BuildBamIndex",
            &format!("I={}", input_bam.display()),
            &format!("O={}", explicit_bai.display()),
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();
    assert!(explicit_bai.exists());
}

#[test]
fn buildbamindex_accepts_create_md5_file_without_index_sidecar() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let explicit_bai = tempdir.path().join("explicit.bai");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SORT_ORDER=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "BuildBamIndex",
            &format!("I={}", input_bam.display()),
            &format!("O={}", explicit_bai.display()),
            "CREATE_MD5_FILE=true",
        ])
        .assert()
        .success();

    assert!(explicit_bai.exists());
    assert!(!tempdir.path().join("explicit.bai.md5").exists());
}

#[test]
fn buildbamindex_delegates_unsupported_sam_input_to_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);
    let log = tempdir.path().join("fallback.args");
    let input_sam = tempdir.path().join("input.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env(
            "TURBO_PICARD_FALLBACK_COMMAND",
            fallback.display().to_string(),
        )
        .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
        .args(["BuildBamIndex", &format!("I={}", input_sam.display())])
        .assert()
        .success();

    assert!(log.exists());
}

#[test]
fn mergesamfiles_merges_and_sorts_by_coordinate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "read-b\t0\tchr1\t50\t60\t4M\t*\t0\t0\tBBBB\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
        "CO=merged by turbo-picard",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@HD\tVN:1.6\tSO:coordinate"));
    assert!(output_sam.contains("@CO\tmerged by turbo-picard"));
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn mergesamfiles_falls_back_to_full_sort_for_unsorted_inputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-b\t0\tchr1\t50\t60\t4M\t*\t0\t0\tBBBB\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn mergesamfiles_unsorted_fallback_uses_bounded_temp_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    let sort_tmp = tempdir.path().join("merge-tmp");
    fs::create_dir(&sort_tmp).expect("sort tmp exists");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "late\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
            "dup\t0\tchr1\t20\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "dup\t0\tchr1\t20\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
            "early\t0\tchr1\t10\t60\t4M\t*\t0\t0\tGGGG\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeSamFiles",
            &format!("I={}", input_a.display()),
            &format!("I={}", input_b.display()),
            &format!("O={}", output.display()),
            "SORT_ORDER=coordinate",
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
        ])
        .assert()
        .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(
        record_names(&output_sam),
        vec!["early", "dup", "dup", "late"]
    );
    assert_eq!(
        output_sam
            .lines()
            .filter(|line| !line.starts_with('@'))
            .map(|line| line.split('\t').nth(9).expect("sequence field"))
            .collect::<Vec<_>>(),
        vec!["GGGG", "AAAA", "TTTT", "CCCC"]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("merge tmp readable")
            .next()
            .is_none(),
        "MergeSamFiles fallback should clean temporary runs"
    );
}

#[test]
fn mergesamfiles_assume_sorted_uses_streaming_merge() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "read-b\t0\tchr1\t50\t60\t4M\t*\t0\t0\tBBBB\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
        "SORT_ORDER=coordinate",
        "AS=true",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(
        record_names(&output_sam),
        vec!["read-a", "read-b", "read-c"]
    );
}

#[test]
fn mergesamfiles_filters_records_by_intervals() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let intervals = tempdir.path().join("targets.interval_list");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t10M\t*\t0\t0\tCCCCCCCCCC\tFFFFFFFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t50\t60\t10M\t*\t0\t0\tBBBBBBBBBB\tFFFFFFFFFF\n",
        ),
    )
    .expect("second input fixture is written");
    fs::write(
        &intervals,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t45\t60\t+\ttarget\n",
        ),
    )
    .expect("interval fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeSamFiles",
            &format!("I={}", input_a.display()),
            &format!("I={}", input_b.display()),
            &format!("O={}", output.display()),
            "SORT_ORDER=coordinate",
            "AS=true",
            &format!("INTERVALS={}", intervals.display()),
        ])
        .assert()
        .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(record_names(&output_sam), vec!["read-b"]);
}

#[test]
fn mergesamfiles_preserves_unsorted_input_order() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t90\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
        "SO=unsorted",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@HD\tVN:1.6\tSO:unsorted"));
    assert_eq!(record_names(&output_sam), vec!["read-c", "read-a"]);
}

#[test]
fn mergesamfiles_rewrites_colliding_read_groups() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tLB:lib-a\tPL:ILLUMINA\tSM:sample-a\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tLB:lib-b\tPL:ILLUMINA\tSM:sample-b\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tBBBB\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("@RG\tID:rg1\tLB:lib-a"));
    assert!(output_sam.contains("@RG\tID:rg1.1\tLB:lib-b"));
    assert!(output_sam.contains("read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\tRG:Z:rg1"));
    assert!(output_sam.contains("read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tBBBB\tFFFF\tRG:Z:rg1.1"));
}

#[test]
fn mergesamfiles_accepts_merge_sequence_dictionaries_for_matching_dictionaries() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.sam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tBBBB\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeSamFiles",
            &format!("I={}", input_a.display()),
            &format!("I={}", input_b.display()),
            &format!("O={}", output.display()),
            "SORT_ORDER=coordinate",
            "MERGE_SEQUENCE_DICTIONARIES=true",
        ])
        .assert()
        .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert_eq!(record_names(&output_sam), vec!["read-a", "read-b"]);
}

#[test]
fn mergesamfiles_writes_requested_bam_sidecars() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_a = tempdir.path().join("a.sam");
    let input_b = tempdir.path().join("b.sam");
    let output = tempdir.path().join("merged.bam");
    fs::write(
        &input_a,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tBBBB\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &input_b,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MergeSamFiles",
        &format!("I={}", input_a.display()),
        &format!("I={}", input_b.display()),
        &format!("O={}", output.display()),
        "CREATE_MD5_FILE=true",
        "CREATE_INDEX=true",
        "COMPRESSION_LEVEL=5",
        "MAX_RECORDS_IN_RAM=500",
        &format!("TMP_DIR={}", tempdir.path().display()),
        "VERBOSITY=WARNING",
    ])
    .assert()
    .success();

    assert!(output.exists());
    assert!(tempdir.path().join("merged.bam.md5").exists());
    assert!(tempdir.path().join("merged.bai").exists());
}

#[test]
fn samtofastq_streams_unpaired_reads() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t16\tchr1\t10\t60\t4M\t*\t0\t0\tAACG\tABCD\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("FASTQ={}", fastq.display()),
    ])
    .assert()
    .success();

    let output_fastq = fs::read_to_string(&fastq).expect("FASTQ output exists");
    assert_eq!(
        output_fastq,
        concat!(
            "@read-a\n",
            "ACGT\n",
            "+\n",
            "FFFF\n",
            "@read-b\n",
            "CGTT\n",
            "+\n",
            "DCBA\n",
        )
    );
}

#[test]
fn samtofastq_honors_re_reverse_false() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t16\tchr1\t10\t60\t4M\t*\t0\t0\tAACG\tABCD\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", fastq.display()),
            "RE_REVERSE=false",
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&fastq).expect("FASTQ output exists"),
        "@read-a\nAACG\n+\nABCD\n"
    );
}

#[test]
fn samtofastq_splits_paired_reads() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let first_fastq = tempdir.path().join("r1.fastq");
    let second_fastq = tempdir.path().join("r2.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "orphan-a\t77\t*\t0\t0\t*\t*\t0\t0\tCCCC\tIIII\n",
            "pair-a\t77\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
            "pair-a\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tHHHH\n",
            "orphan-b\t141\t*\t0\t0\t*\t*\t0\t0\tGGGG\tJJJJ\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("F={}", first_fastq.display()),
        &format!("F2={}", second_fastq.display()),
        "VALIDATION_STRINGENCY=SILENT",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(&first_fastq).expect("first FASTQ exists"),
        "@pair-a/1\nAAAA\n+\nFFFF\n"
    );
    assert_eq!(
        fs::read_to_string(&second_fastq).expect("second FASTQ exists"),
        "@pair-a/2\nTTTT\n+\nHHHH\n"
    );
}

#[test]
fn samtofastq_applies_read_trimming_and_max_bases() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let first_fastq = tempdir.path().join("r1.fastq");
    let second_fastq = tempdir.path().join("r2.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair-a\t77\t*\t0\t0\t*\t*\t0\t0\tAACCGG\tABCDEF\n",
            "pair-a\t141\t*\t0\t0\t*\t*\t0\t0\tTTGGCC\tUVWXYZ\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("FASTQ={}", first_fastq.display()),
        &format!("SECOND_END_FASTQ={}", second_fastq.display()),
        "READ1_TRIM=1",
        "READ1_MAX_BASES_TO_WRITE=3",
        "READ2_TRIM=2",
        "READ2_MAX_BASES_TO_WRITE=2",
        "VALIDATION_STRINGENCY=SILENT",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(&first_fastq).expect("first FASTQ exists"),
        "@pair-a/1\nACC\n+\nBCD\n"
    );
    assert_eq!(
        fs::read_to_string(&second_fastq).expect("second FASTQ exists"),
        "@pair-a/2\nGG\n+\nWX\n"
    );
}

#[test]
fn samtofastq_applies_quality_trimming() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFF!!!\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tTGCA\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("FASTQ={}", fastq.display()),
        "Q=20",
        "VALIDATION_STRINGENCY=SILENT",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(&fastq).expect("FASTQ exists"),
        concat!("@read-a\nACG\n+\nFFF\n", "@read-b\nT\n+\n!\n")
    );
}

#[test]
fn samtofastq_applies_clipping_attribute_actions() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let masked = tempdir.path().join("masked.fastq");
    let trimmed = tempdir.path().join("trimmed.fastq");
    let quality = tempdir.path().join("quality.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tAACCGG\tFFFFFF\tXT:i:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", masked.display()),
            "CLIP_ATTR=XT",
            "CLIP_ACT=N",
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", trimmed.display()),
            "CLIPPING_ATTRIBUTE=XT",
            "CLIPPING_ACTION=X",
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", quality.display()),
            "CLIPPING_ATTRIBUTE=XT",
            "CLIPPING_ACTION=2",
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&masked).expect("masked FASTQ exists"),
        "@read-a\nAACNNN\n+\nFFFFFF\n"
    );
    assert_eq!(
        fs::read_to_string(&trimmed).expect("trimmed FASTQ exists"),
        "@read-a\nAAC\n+\nFFF\n"
    );
    assert_eq!(
        fs::read_to_string(&quality).expect("quality FASTQ exists"),
        "@read-a\nAACCGG\n+\nFFF###\n"
    );
}

#[test]
fn samtofastq_writes_gzip_fastq_outputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq.gz");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("FASTQ={}", fastq.display()),
        "COMPRESSION_LEVEL=1",
    ])
    .assert()
    .success();

    let compressed = fs::File::open(&fastq).expect("gzip FASTQ exists");
    let mut decoder = GzDecoder::new(compressed);
    let mut output_fastq = String::new();
    decoder
        .read_to_string(&mut output_fastq)
        .expect("FASTQ is gzip-compressed");
    assert_eq!(output_fastq, "@read-a\nACGT\n+\nFFFF\n");
}

#[test]
fn samtofastq_filters_non_pf_and_non_primary_by_default() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let default_fastq = tempdir.path().join("default.fastq");
    let included_fastq = tempdir.path().join("included.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "primary\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
            "nonpf\t516\t*\t0\t0\t*\t*\t0\t0\tCCCC\tFFFF\n",
            "secondary\t260\t*\t0\t0\t*\t*\t0\t0\tGGGG\tFFFF\n",
            "supp\t2048\tchr1\t1\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", default_fastq.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&default_fastq).expect("default FASTQ exists"),
        "@primary\nAAAA\n+\nFFFF\n"
    );

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", included_fastq.display()),
            "INCLUDE_NON_PF_READS=true",
            "INCLUDE_NON_PRIMARY_ALIGNMENTS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let included = fs::read_to_string(included_fastq).expect("included FASTQ exists");
    assert!(included.contains("@primary\nAAAA\n+\nFFFF\n"));
    assert!(included.contains("@nonpf\nCCCC\n+\nFFFF\n"));
    assert!(included.contains("@secondary\nGGGG\n+\nFFFF\n"));
    assert!(included.contains("@supp\nTTTT\n+\nFFFF\n"));
}

#[test]
fn samtofastq_writes_md5_sidecar_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    let md5_path = tempdir.path().join("reads.fastq.md5");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", fastq.display()),
            "CREATE_MD5_FILE=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output_bytes = fs::read(&fastq).expect("FASTQ output exists");
    let md5 = fs::read_to_string(&md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5, format!("{:x}", md5::compute(output_bytes)));
}

#[test]
fn samtofastq_accepts_common_runtime_sidecar_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    let reference = tempdir.path().join("ref.fa");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", fastq.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            &format!("R={}", reference.display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&fastq).expect("FASTQ output exists"),
        "@read-a\nACGT\n+\nFFFF\n"
    );
    assert!(tempdir.path().join("reads.fastq.md5").exists());
    assert!(!tempdir.path().join("reads.fastq.bai").exists());
    assert!(!tempdir.path().join("reads.bai").exists());
}

#[test]
fn samtofastq_rejects_paired_reads_without_second_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let fastq = tempdir.path().join("reads.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "pair\t77\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
            "pair\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", fastq.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "SamToFastq input contains paired reads but no SECOND_END_FASTQ was specified",
        ));
}

#[test]
fn samtofastq_writes_unpaired_fastq_without_second_end_fastq() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let first_fastq = tempdir.path().join("reads.fastq");
    let unpaired_fastq = tempdir.path().join("unpaired.fastq");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
            "read-b\t16\tchr1\t10\t60\t4M\t*\t0\t0\tAACG\tABCD\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", first_fastq.display()),
            &format!("UNPAIRED_FASTQ={}", unpaired_fastq.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&first_fastq).expect("FASTQ exists"), "");
    assert_eq!(
        fs::read_to_string(&unpaired_fastq).expect("unpaired FASTQ exists"),
        concat!(
            "@read-a\n",
            "AAAA\n",
            "+\n",
            "FFFF\n",
            "@read-b\n",
            "CGTT\n",
            "+\n",
            "DCBA\n",
        )
    );
}

#[test]
fn samtofastq_rejects_picard_invalid_option_combinations() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", tempdir.path().join("bad-clip.fastq").display()),
            "CLIPPING_ATTRIBUTE=XT",
            "CLIPPING_ACTION=bad",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported SamToFastq CLIPPING_ACTION",
        ));

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            &format!("FASTQ={}", tempdir.path().join("bad-per-rg.fastq").display()),
            "OUTPUT_PER_RG=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "SamToFastq OUTPUT_PER_RG cannot be combined with FASTQ, SECOND_END_FASTQ, or UNPAIRED_FASTQ",
        ));
}

#[test]
fn samtofastq_can_output_fastqs_per_read_group_using_platform_unit() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output_dir = tempdir.path().join("per-rg");
    fs::create_dir(&output_dir).expect("output dir exists");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\tPU:unit 1\n",
            "pair1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t141\t*\t0\t0\t*\t*\t0\t0\tTGCA\tIIII\tRG:Z:rg1\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            "OUTPUT_PER_RG=true",
            &format!("OUTPUT_DIR={}", output_dir.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output_dir.join("unit_1_1.fastq")).expect("read1 FASTQ exists"),
        "@pair1/1\nACGT\n+\nFFFF\n",
    );
    assert_eq!(
        fs::read_to_string(output_dir.join("unit_1_2.fastq")).expect("read2 FASTQ exists"),
        "@pair1/2\nTGCA\n+\nIIII\n",
    );
}

#[test]
fn samtofastq_can_output_compressed_fastqs_per_read_group_using_id() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output_dir = tempdir.path().join("per-rg");
    fs::create_dir(&output_dir).expect("output dir exists");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg:1\tSM:sample\tLB:lib\tPL:ILLUMINA\tPU:unit1\n",
            "pair1\t77\t*\t0\t0\t*\t*\t0\t0\tAAAA\tHHHH\tRG:Z:rg:1\n",
            "pair1\t141\t*\t0\t0\t*\t*\t0\t0\tCCCC\tJJJJ\tRG:Z:rg:1\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input.display()),
            "OUTPUT_PER_RG=true",
            "RG_TAG=ID",
            "COMPRESS_OUTPUTS_PER_RG=true",
            &format!("OUTPUT_DIR={}", output_dir.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let read1 = read_gzip_to_string(output_dir.join("rg_1_1.fastq.gz"));
    let read2 = read_gzip_to_string(output_dir.join("rg_1_2.fastq.gz"));
    assert_eq!(read1, "@pair1/1\nAAAA\n+\nHHHH\n");
    assert_eq!(read2, "@pair1/2\nCCCC\n+\nJJJJ\n");
}

#[test]
fn samtofastq_can_output_fastqs_per_read_group_from_bam_input() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let output_dir = tempdir.path().join("per-rg");
    fs::create_dir(&output_dir).expect("output dir exists");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\tPU:lane/1\n",
            "pair1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t141\t*\t0\t0\t*\t*\t0\t0\tTGCA\tIIII\tRG:Z:rg1\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=queryname",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SamToFastq",
            &format!("I={}", input_bam.display()),
            "OUTPUT_PER_RG=true",
            &format!("OUTPUT_DIR={}", output_dir.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output_dir.join("lane_1_1.fastq")).expect("read1 FASTQ exists"),
        "@pair1/1\nACGT\n+\nFFFF\n",
    );
    assert_eq!(
        fs::read_to_string(output_dir.join("lane_1_2.fastq")).expect("read2 FASTQ exists"),
        "@pair1/2\nTGCA\n+\nIIII\n",
    );
}

#[test]
fn fastqtosam_writes_unmapped_paired_sam_with_read_group() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let r1 = tempdir.path().join("r1.fastq");
    let r2 = tempdir.path().join("r2.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(
        &r1,
        concat!(
            "@read1\n", "ACGT\n", "+\n", "FFFF\n", "@read2\n", "TGCA\n", "+\n", "EEEE\n",
        ),
    )
    .expect("r1 fixture is written");
    fs::write(
        &r2,
        concat!(
            "@read1\n", "TTTT\n", "+\n", "IIII\n", "@read2\n", "CCCC\n", "+\n", "HHHH\n",
        ),
    )
    .expect("r2 fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("F1={}", r1.display()),
            &format!("F2={}", r2.display()),
            &format!("O={}", output.display()),
            "SM=sample",
            "RG=rg1",
            "LB=lib",
            "PL=ILLUMINA",
            "PU=unit",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("@HD\tVN:1.6\tSO:queryname\n"));
    assert!(sam.contains("@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\tPU:unit\n"));
    assert!(sam.contains("read1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n"));
    assert!(sam.contains("read1\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tIIII\tRG:Z:rg1\n"));
    assert!(sam.contains("read2\t77\t*\t0\t0\t*\t*\t0\t0\tTGCA\tEEEE\tRG:Z:rg1\n"));
    assert!(sam.contains("read2\t141\t*\t0\t0\t*\t*\t0\t0\tCCCC\tHHHH\tRG:Z:rg1\n"));
}

#[test]
fn fastqtosam_reads_gzip_single_end_fastq() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq.gz");
    let output = tempdir.path().join("unmapped.sam");
    {
        let file = fs::File::create(&fastq).expect("gzip FASTQ can be created");
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder
            .write_all(b"@read1\nACGT\n+\nFFFF\n")
            .expect("gzip FASTQ fixture is written");
        encoder.finish().expect("gzip FASTQ is finished");
    }

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "READ_GROUP_NAME=rg1",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n"));
}

#[test]
fn fastqtosam_writes_md5_sidecar_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    let md5_path = tempdir.path().join("unmapped.sam.md5");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "READ_GROUP_NAME=rg1",
            "CREATE_MD5_FILE=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output_bytes = fs::read(&output).expect("SAM output exists");
    let md5 = fs::read_to_string(&md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5, format!("{:x}", md5::compute(output_bytes)));
}

#[test]
fn fastqtosam_accepts_common_runtime_sidecar_options_for_sam_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    let reference = tempdir.path().join("ref.fa");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "READ_GROUP_NAME=rg1",
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            &format!("R={}", reference.display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("unmapped.sam.md5").exists());
    assert!(!tempdir.path().join("unmapped.sam.bai").exists());
    assert!(!tempdir.path().join("unmapped.bai").exists());
}

#[test]
fn fastqtosam_writes_unsorted_header_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "SORT_ORDER=unsorted",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("@HD\tVN:1.6\tSO:unsorted\n"));
    assert!(sam.contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:A\n"));
}

#[test]
fn fastqtosam_writes_coordinate_header_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "SORT_ORDER=coordinate",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("@HD\tVN:1.6\tSO:coordinate\n"));
    assert!(sam.contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:A\n"));
}

#[test]
fn fastqtosam_writes_repeated_comments_to_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "COMMENT=first comment",
            "COMMENT=second comment",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("@CO\tfirst comment\n@CO\tsecond comment\n"));
    assert!(sam.contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:A\n"));
}

#[test]
fn fastqtosam_auto_detects_standard_and_illumina_quality_offsets() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let standard_fastq = tempdir.path().join("standard.fastq");
    let standard_output = tempdir.path().join("standard.sam");
    let illumina_fastq = tempdir.path().join("illumina.fastq");
    let illumina_output = tempdir.path().join("illumina.sam");
    fs::write(&standard_fastq, "@read1\nACGT\n+\nFFFF\n").expect("standard FASTQ is written");
    fs::write(&illumina_fastq, "@read1\nACGT\n+\nbbbb\n").expect("illumina FASTQ is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", standard_fastq.display()),
            &format!("OUTPUT={}", standard_output.display()),
            "SAMPLE_NAME=sample",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", illumina_fastq.display()),
            &format!("OUTPUT={}", illumina_output.display()),
            "SAMPLE_NAME=sample",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(
        fs::read_to_string(&standard_output)
            .expect("standard output exists")
            .contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t''''\tRG:Z:A\n")
    );
    assert!(
        fs::read_to_string(&illumina_output)
            .expect("illumina output exists")
            .contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tCCCC\tRG:Z:A\n")
    );
}

#[test]
fn fastqtosam_accepts_solexa_quality_format() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("solexa.fastq");
    let output = tempdir.path().join("solexa.sam");
    fs::write(&fastq, "@read1\nACGT\n+\n;@JT\n").expect("solexa FASTQ is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "QUALITY_FORMAT=Solexa",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(
        fs::read_to_string(&output)
            .expect("solexa output exists")
            .contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t\"$+5\tRG:Z:A\n")
    );
}

#[test]
fn fastqtosam_accepts_sequential_fastq_collections() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let r1_001 = tempdir.path().join("reads_R1_001.fastq");
    let r1_002 = tempdir.path().join("reads_R1_002.fastq");
    let r2_001 = tempdir.path().join("reads_R2_001.fastq");
    let r2_002 = tempdir.path().join("reads_R2_002.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&r1_001, "@read1\nACGT\n+\nFFFF\n").expect("r1_001 is written");
    fs::write(&r1_002, "@read2\nTGCA\n+\nEEEE\n").expect("r1_002 is written");
    fs::write(&r2_001, "@read1\nTTTT\n+\nIIII\n").expect("r2_001 is written");
    fs::write(&r2_002, "@read2\nCCCC\n+\nHHHH\n").expect("r2_002 is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", r1_001.display()),
            &format!("FASTQ2={}", r2_001.display()),
            "USE_SEQUENTIAL_FASTQS=true",
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "READ_GROUP_NAME=rg1",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("read1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n"));
    assert!(sam.contains("read1\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tIIII\tRG:Z:rg1\n"));
    assert!(sam.contains("read2\t77\t*\t0\t0\t*\t*\t0\t0\tTGCA\tEEEE\tRG:Z:rg1\n"));
    assert!(sam.contains("read2\t141\t*\t0\t0\t*\t*\t0\t0\tCCCC\tHHHH\tRG:Z:rg1\n"));
}

#[test]
fn fastqtosam_rejects_mismatched_sequential_fastq_collections() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let r1_001 = tempdir.path().join("reads_R1_001.fastq");
    let r1_002 = tempdir.path().join("reads_R1_002.fastq");
    let r2_001 = tempdir.path().join("reads_R2_001.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&r1_001, "@read1\nACGT\n+\nFFFF\n").expect("r1_001 is written");
    fs::write(&r1_002, "@read2\nTGCA\n+\nEEEE\n").expect("r1_002 is written");
    fs::write(&r2_001, "@read1\nTTTT\n+\nIIII\n").expect("r2_001 is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", r1_001.display()),
            &format!("FASTQ2={}", r2_001.display()),
            "USE_SEQUENTIAL_FASTQS=true",
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Found 2 files for FASTQ and 1 files for FASTQ2.",
        ));
}

#[test]
fn fastqtosam_honors_empty_line_and_empty_fastq_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&fastq, "\n@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "ALLOW_AND_IGNORE_EMPTY_LINES=true",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let sam = fs::read_to_string(&output).expect("SAM output exists");
    assert!(sam.contains("read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:A\n"));

    let empty_fastq = tempdir.path().join("empty.fastq");
    let empty_output = tempdir.path().join("empty.sam");
    fs::write(&empty_fastq, "").expect("empty FASTQ fixture is written");
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", empty_fastq.display()),
            &format!("OUTPUT={}", empty_output.display()),
            "SAMPLE_NAME=sample",
            "ALLOW_EMPTY_FASTQ=true",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(
        fs::read_to_string(&empty_output)
            .expect("empty SAM output exists")
            .contains("@HD\tVN:1.6\tSO:queryname\n")
    );
}

#[test]
fn fastqtosam_rejects_empty_fastq_and_out_of_range_quality() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let empty_fastq = tempdir.path().join("empty.fastq");
    let empty_output = tempdir.path().join("empty.sam");
    fs::write(&empty_fastq, "").expect("empty FASTQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", empty_fastq.display()),
            &format!("OUTPUT={}", empty_output.display()),
            "SAMPLE_NAME=sample",
            "QUALITY_FORMAT=Standard",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty FASTQ input"));

    let fastq = tempdir.path().join("reads.fastq");
    let output = tempdir.path().join("unmapped.sam");
    fs::write(&fastq, "@read1\nACGT\n+\nFFFF\n").expect("FASTQ fixture is written");
    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FastqToSam",
            &format!("FASTQ={}", fastq.display()),
            &format!("OUTPUT={}", output.display()),
            "SAMPLE_NAME=sample",
            "QUALITY_FORMAT=Standard",
            "MAX_Q=30",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("quality above MAX_Q"));
}

#[test]
fn addorreplacereadgroups_rewrites_header_and_record_tags() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:old\tLB:old-lib\tPL:ILLUMINA\tPU:old-unit\tSM:old-sample\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:old\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "AddOrReplaceReadGroups",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "RGID=new",
        "RGLB=library-a",
        "RGPL=ILLUMINA",
        "RGPU=unit-a",
        "RGSM=sample-a",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(!output_sam.contains("ID:old"));
    assert!(
        output_sam.contains("@RG\tID:new\tLB:library-a\tPL:ILLUMINA\tSM:sample-a\tPU:unit-a\n")
    );
    assert!(output_sam.contains("read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:new"));
}

#[test]
fn addorreplacereadgroups_writes_flow_order_and_key_sequence() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "AddOrReplaceReadGroups",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "RGID=new",
        "RGLB=library-a",
        "RGPL=ILLUMINA",
        "RGPU=unit-a",
        "RGSM=sample-a",
        "RGKS=ACGT",
        "RGFO=TACG",
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains(
        "@RG\tID:new\tLB:library-a\tPL:ILLUMINA\tSM:sample-a\tPU:unit-a\tKS:ACGT\tFO:TACG\n"
    ));
    assert!(output_sam.contains("read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:new"));
}

#[test]
fn addorreplacereadgroups_writes_md5_sidecar_but_no_index_for_sam_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let reference = tempdir.path().join("ref.fa");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "AddOrReplaceReadGroups",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "RGID=new",
            "RGLB=library-a",
            "RGPL=ILLUMINA",
            "RGPU=unit-a",
            "RGSM=sample-a",
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            &format!("R={}", reference.display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.exists());
    let md5 =
        fs::read_to_string(format!("{}.md5", output.display())).expect("SAM md5 sidecar exists");
    assert_eq!(md5.len(), 32);
    assert!(md5.chars().all(|char| char.is_ascii_hexdigit()));
    assert!(!tempdir.path().join("output.sam.bai").exists());
    assert!(!tempdir.path().join("output.bai").exists());
}

#[test]
fn collectalignmentsummarymetrics_writes_unpaired_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\n",
            "read-c\t16\tchr1\t20\t30\t4M\t*\t0\t0\tAACG\tABCD\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CollectAlignmentSummaryMetrics",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## METRICS CLASS\tpicard.analysis.AlignmentSummaryMetrics\n"));
    assert!(metrics.contains(
        "UNPAIRED\t3\t3\t1\t0\t2\t0.666667\t8\t2\t8\t8\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2.666667\t0\t0\t0\t0\t0\t0.5\t0\t0\t0\t0\t0\t\t\t\n"
    ));
    assert!(
        metrics
            .contains("READ_LENGTH\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n")
    );
    assert!(metrics.contains("0\t0\t1\n"));
    assert!(metrics.contains("4\t3\t2\n"));
}

#[test]
fn collectalignmentsummarymetrics_reports_cigar_and_chimera_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000000\n",
            "chimera\t97\tchr1\t1\t60\t4M\t=\t200000\t200000\tACGT\tFFFF\n",
            "softclip\t99\tchr1\t20\t60\t3M1S\t=\t100\t100\tACGT\tFFFF\n",
            "chimera\t145\tchr1\t200000\t60\t3M1D1M\t=\t1\t-200000\tACGT\tFFFF\n",
            "softclip\t147\tchr1\t100\t60\t4M\t=\t20\t-100\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    let first = metrics
        .lines()
        .find(|line| line.starts_with("FIRST_OF_PAIR\t"))
        .expect("first-of-pair row exists")
        .split('\t')
        .collect::<Vec<_>>();
    let second = metrics
        .lines()
        .find(|line| line.starts_with("SECOND_OF_PAIR\t"))
        .expect("second-of-pair row exists")
        .split('\t')
        .collect::<Vec<_>>();
    let pair = metrics
        .lines()
        .find(|line| line.starts_with("PAIR\t"))
        .expect("pair row exists")
        .split('\t')
        .collect::<Vec<_>>();

    assert_eq!(first[28], "0.5");
    assert_eq!(first[30], "0.125");
    assert_eq!(first[32], "1");
    assert_eq!(second[14], "0.125");
    assert_eq!(second[28], "0.5");
    assert_eq!(pair[14], "0.066667");
    assert_eq!(pair[28], "0.5");
    assert_eq!(pair[30], "0.0625");
    assert!(
        metrics.contains("READ_LENGTH\tPAIRED_TOTAL_LENGTH_COUNT\tPAIRED_ALIGNED_LENGTH_COUNT\n")
    );
}

#[test]
fn collectalignmentsummarymetrics_ignores_secondary_and_supplementary_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t99\tchr1\t1\t60\t4M\t=\t20\t20\tACGT\tFFFF\n",
            "primary\t147\tchr1\t20\t60\t4M\t=\t1\t-20\tTGCA\tFFFF\n",
            "secondary\t355\tchr1\t30\t60\t4M\t=\t50\t20\tAAAA\tFFFF\n",
            "supplementary\t2147\tchr1\t50\t60\t4M\t=\t30\t-20\tCCCC\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    let pair = metrics
        .lines()
        .find(|line| line.starts_with("PAIR\t"))
        .expect("pair row exists")
        .split('\t')
        .collect::<Vec<_>>();

    assert_eq!(pair[1], "2");
    assert_eq!(pair[2], "2");
    assert!(metrics.contains("4\t2\t2\n"));
}

#[test]
fn collectalignmentsummarymetrics_counts_bad_cycles_per_pair_end() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t77\t*\t0\t0\t*\t*\t0\t0\tANAA\tFFFF\n",
            "pair1\t141\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
            "pair2\t77\t*\t0\t0\t*\t*\t0\t0\tANAA\tFFFF\n",
            "pair2\t141\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
            "pair3\t77\t*\t0\t0\t*\t*\t0\t0\tANAA\tFFFF\n",
            "pair3\t141\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
            "pair4\t77\t*\t0\t0\t*\t*\t0\t0\tANAA\tFFFF\n",
            "pair4\t141\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
            "pair5\t77\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
            "pair5\t141\t*\t0\t0\t*\t*\t0\t0\tACAA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    let first = metrics
        .lines()
        .find(|line| line.starts_with("FIRST_OF_PAIR\t"))
        .expect("first-of-pair row exists")
        .split('\t')
        .collect::<Vec<_>>();
    let second = metrics
        .lines()
        .find(|line| line.starts_with("SECOND_OF_PAIR\t"))
        .expect("second-of-pair row exists")
        .split('\t')
        .collect::<Vec<_>>();
    let pair = metrics
        .lines()
        .find(|line| line.starts_with("PAIR\t"))
        .expect("pair row exists")
        .split('\t')
        .collect::<Vec<_>>();

    assert_eq!(first[26], "1");
    assert_eq!(second[26], "0");
    assert_eq!(pair[26], "1");
}

#[test]
fn collectalignmentsummarymetrics_counts_default_adapter_reads() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "adapter\t4\t*\t0\t0\t*\t*\t0\t0\tGATCGGAAGAGCACACGTCT\tFFFFFFFFFFFFFFFFFFFF\n",
            "genomic\t4\t*\t0\t0\t*\t*\t0\t0\tACGTACGTACGTACGTACGT\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    let unpaired = metrics
        .lines()
        .find(|line| line.starts_with("UNPAIRED\t"))
        .expect("unpaired row exists")
        .split('\t')
        .collect::<Vec<_>>();

    assert_eq!(unpaired[29], "0.5");
}

#[test]
fn collectalignmentsummarymetrics_accepts_metric_level_short_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "LEVEL=ALL_READS",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CATEGORY\tTOTAL_READS"));
}

#[test]
fn collectalignmentsummarymetrics_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CATEGORY\tTOTAL_READS"));
}

#[test]
fn collectalignmentsummarymetrics_honors_stop_after_and_assume_sorted_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "STOP_AFTER=1",
            "AS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("UNPAIRED\t1\t1\t1\t0\t1\t1"));
    assert!(metrics.contains("\t4\t?\t4\t0\t4\t4\t4\t0\t0"));
    assert!(metrics.contains(
        "READ_LENGTH\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n4\t1\t1\n"
    ));
    assert!(!metrics.contains("\n0\t0\t1\n"));
}

#[test]
fn collectalignmentsummarymetrics_can_accumulate_by_sample() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "METRIC_ACCUMULATION_LEVEL=SAMPLE",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "UNPAIRED\t2\t2\t1\t0\t1\t0.5\t4\t1\t4\t4\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\t\t\t\n",
    ));
    assert!(metrics.contains(
        "UNPAIRED\t2\t2\t1\t0\t1\t0.5\t4\t1\t4\t4\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\tsampleA\t\t\n",
    ));
}

#[test]
fn collectalignmentsummarymetrics_can_accumulate_by_library() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "METRIC_ACCUMULATION_LEVEL=LIBRARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "UNPAIRED\t2\t2\t1\t0\t1\t0.5\t4\t1\t4\t4\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\tsampleA\tlib1\t\n",
    ));
}

#[test]
fn collectalignmentsummarymetrics_can_accumulate_by_read_group() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("alignment_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectAlignmentSummaryMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "METRIC_ACCUMULATION_LEVEL=READ_GROUP",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "UNPAIRED\t2\t2\t1\t0\t1\t0.5\t4\t1\t4\t4\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\tsampleA\tlib1\tunit1\n",
    ));
}

#[test]
fn collectqualityyieldmetrics_writes_quality_yield_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_yield_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t512\tchr1\t20\t60\t4M\t*\t0\t0\tNNNN\t!!!!\n",
            "read-c\t256\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CollectQualityYieldMetrics",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "## METRICS CLASS\tpicard.analysis.CollectQualityYieldMetrics$QualityYieldMetrics\n"
    ));
    assert!(metrics.contains(
        "TOTAL_READS\tPF_READS\tREAD_LENGTH\tTOTAL_BASES\tPF_BASES\tQ20_BASES\tPF_Q20_BASES\tQ30_BASES\tPF_Q30_BASES\tQ20_EQUIVALENT_YIELD\tPF_Q20_EQUIVALENT_YIELD\n"
    ));
    assert!(metrics.contains("2\t1\t4\t8\t4\t4\t4\t4\t4\t7\t7\n"));
}

#[test]
fn collectqualityyieldmetrics_honors_stop_after() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_yield_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectQualityYieldMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "STOP_AFTER=1",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("1\t1\t4\t4\t4\t4\t4\t4\t4\t7\t7\n"));
}

#[test]
fn collectqualityyieldmetrics_uses_original_qualities_by_default() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_yield_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectQualityYieldMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("1\t1\t4\t4\t4\t4\t4\t4\t4\t7\t7\n"));
}

#[test]
fn collectqualityyieldmetrics_can_disable_original_qualities_in_sam() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_yield_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectQualityYieldMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "USE_ORIGINAL_QUALITIES=false",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("1\t1\t4\t4\t4\t0\t0\t0\t0\t0\t0\n"));
}

#[test]
fn collectqualityyieldmetrics_can_include_secondary_and_supplemental_alignments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_yield_metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "primary\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "secondary\t256\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "supplemental\t2048\tchr1\t3\t60\t4M\t*\t0\t0\tACGT\tEEEE\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectQualityYieldMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "INCLUDE_SECONDARY_ALIGNMENTS=true",
            "INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("3\t3\t4\t12\t12\t12\t12\t12\t12\t22\t22\n"));
}

#[test]
fn collectbasedistributionbycycle_writes_base_percentages() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("base_distribution.txt");
    let chart = tempdir.path().join("base_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t2\t60\t4M\t*\t0\t0\tAAGT\tFFFF\n",
            "read-c\t16\tchr1\t3\t60\t4M\t*\t0\t0\tNNGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectBaseDistributionByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## METRICS CLASS\tpicard.analysis.BaseDistributionByCycleMetrics\n"));
    assert!(metrics.contains("READ_END\tCYCLE\tPCT_A\tPCT_C\tPCT_G\tPCT_T\tPCT_N\n"));
    assert!(metrics.contains("1\t1\t66.666667\t0\t0\t33.333333\t0\n"));
    assert!(metrics.contains("1\t2\t33.333333\t33.333333\t33.333333\t0\t0\n"));
    assert!(chart.exists());
}

#[test]
fn collectbasedistributionbycycle_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("base_distribution.txt");
    let chart = tempdir.path().join("base_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectBaseDistributionByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("READ_END\tCYCLE\tPCT_A\tPCT_C\tPCT_G\tPCT_T\tPCT_N\n"));
}

#[test]
fn collectbasedistributionbycycle_honors_stop_after_and_assume_sorted_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("base_distribution.txt");
    let chart = tempdir.path().join("base_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t2\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectBaseDistributionByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "STOP_AFTER=1",
            "AS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("1\t1\t100\t0\t0\t0\t0\n"));
    assert!(metrics.contains("1\t2\t0\t100\t0\t0\t0\n"));
    assert!(!metrics.contains("50.000000"));
    assert!(chart.exists());
}

#[test]
fn collectgcbiasmetrics_writes_detail_summary_and_chart() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let detail = tempdir.path().join("gc_bias.detail.txt");
    let summary = tempdir.path().join("gc_bias.summary.txt");
    let chart = tempdir.path().join("gc_bias.pdf");
    fs::write(
        &reference,
        concat!(
            ">low\n",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            ">high\n",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n",
        ),
    )
    .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:low\tLN:40\n",
            "@SQ\tSN:high\tLN:40\n",
            "low1\t0\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\tFFFFFFFFFFFFFFFFFFFF\n",
            "high1\t0\thigh\t1\t60\t20M\t*\t0\t0\tCCCCCCCCCCCCCCCCCCCC\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectGcBiasMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", detail.display()),
            &format!("S={}", summary.display()),
            &format!("CHART={}", chart.display()),
            &format!("R={}", reference.display()),
            "SCAN_WINDOW_SIZE=20",
            "MINIMUM_GENOME_FRACTION=0",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let detail_text = fs::read_to_string(&detail).expect("detail metrics exist");
    assert!(detail_text.contains("## METRICS CLASS\tpicard.analysis.GcBiasDetailMetrics\n"));
    assert!(detail_text.contains("All Reads\tALL\t0\t19\t1\t37\t1\t1\t\t\t\n"));
    assert!(detail_text.contains("All Reads\tALL\t100\t19\t1\t37\t1\t1\t\t\t\n"));
    let summary_text = fs::read_to_string(&summary).expect("summary metrics exist");
    assert!(summary_text.contains("## METRICS CLASS\tpicard.analysis.GcBiasSummaryMetrics\n"));
    assert!(summary_text.contains("All Reads\tALL\t20\t2\t2\t0\t0\t1\t0\t0\t0\t1\t\t\t\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn collectgcbiasmetrics_honors_stop_after_and_assume_sorted_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let detail = tempdir.path().join("gc_bias.detail.txt");
    let summary = tempdir.path().join("gc_bias.summary.txt");
    let chart = tempdir.path().join("gc_bias.pdf");
    fs::write(
        &reference,
        concat!(
            ">low\n",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            ">high\n",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n",
        ),
    )
    .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:low\tLN:40\n",
            "@SQ\tSN:high\tLN:40\n",
            "low1\t0\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\tFFFFFFFFFFFFFFFFFFFF\n",
            "high1\t0\thigh\t1\t60\t20M\t*\t0\t0\tCCCCCCCCCCCCCCCCCCCC\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectGcBiasMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", detail.display()),
            &format!("S={}", summary.display()),
            &format!("CHART={}", chart.display()),
            &format!("R={}", reference.display()),
            "SCAN_WINDOW_SIZE=20",
            "MINIMUM_GENOME_FRACTION=0",
            "STOP_AFTER=1",
            "AS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let detail_text = fs::read_to_string(&detail).expect("detail metrics exist");
    assert!(detail_text.contains("All Reads\tALL\t0\t19\t1\t37\t2\t2\t\t\t\n"));
    assert!(detail_text.contains("All Reads\tALL\t100\t19\t0\t0\t0\t0\t\t\t\n"));
    let summary_text = fs::read_to_string(&summary).expect("summary metrics exist");
    assert!(summary_text.contains("All Reads\tALL\t20\t1\t1\t0\t50\t2\t0\t0\t0\t0\t\t\t\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn collectwgsmetrics_writes_coverage_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## METRICS CLASS\tpicard.analysis.WgsMetrics\n"));
    assert!(metrics.contains(
        "12\t1\t0.603023\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0.833333\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t1\t?\t?\t0.458333\t3\n"
    ));
    assert!(metrics.contains("coverage\thigh_quality_coverage_count\n"));
    assert!(!metrics.contains("coverage\thigh_quality_coverage_count\tunfiltered_baseq_count\n"));
    assert!(metrics.contains("0\t2\n"));
    assert!(metrics.contains("1\t8\n"));
    assert!(metrics.contains("2\t2\n"));
}

#[test]
fn collectwgsmetrics_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## METRICS CLASS\tpicard.analysis.WgsMetrics\n"));
    assert!(metrics.contains("coverage\thigh_quality_coverage_count"));
}

#[test]
fn collectwgsmetrics_honors_stop_after() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t60\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "STOP_AFTER=1",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("1\t1\t?\t0\t0"));
    assert!(metrics.contains("coverage\thigh_quality_coverage_count\n"));
    assert!(metrics.contains("0\t0\n"));
    assert!(metrics.contains("1\t1\n"));
}

#[test]
fn collectwgsmetrics_honors_interval_list_territory() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let intervals = tempdir.path().join("targets.interval_list");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");
    fs::write(
        &intervals,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "chr1\t3\t6\t+\ttarget\n",
        ),
    )
    .expect("interval fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            &format!("INTERVALS={}", intervals.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "4\t1.5\t0.57735\t1.5\t0.5\t0\t0\t0\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t1.5\t1.5\t1.5\t0.625\t4\n"
    ));
    assert!(metrics.contains("0\t0\n"));
    assert!(metrics.contains("1\t2\n"));
    assert!(metrics.contains("2\t2\n"));
}

#[test]
fn collectwgsmetrics_applies_stop_after_to_interval_territory() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let intervals = tempdir.path().join("targets.interval_list");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t60\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");
    fs::write(
        &intervals,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "chr1\t3\t6\t+\ttarget\n",
        ),
    )
    .expect("interval fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            &format!("INTERVALS={}", intervals.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "STOP_AFTER=2",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("2\t2\t0\t2\t0"));
    assert!(metrics.contains("coverage\thigh_quality_coverage_count\n"));
    assert!(metrics.contains("0\t0\n"));
    assert!(metrics.contains("1\t0\n"));
    assert!(metrics.contains("2\t2\n"));
}

#[test]
fn collectwgsmetrics_can_include_base_quality_histogram() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t60\t4M\t*\t0\t0\tGTAC\t!5F?\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\t5555\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=1",
            "INCLUDE_BQ_HISTOGRAM=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("coverage\thigh_quality_coverage_count\tunfiltered_baseq_count\n"));
    assert!(metrics.contains("20\t0\t5\n"));
    assert!(metrics.contains("30\t0\t1\n"));
    assert!(metrics.contains("37\t0\t5\n"));
}

#[test]
fn collectwgsmetrics_handles_sparse_read_on_long_contig() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, format!(">chr1\n{}\n", "A".repeat(100_000)))
        .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100000\n",
            "read-a\t0\tchr1\t99991\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=0",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("100000\t0.00004"));
    assert!(metrics.contains("0\t99996\n"));
    assert!(metrics.contains("1\t4\n"));
}

#[test]
fn collectwgsmetrics_rejects_coordinate_regressions() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("wgs_metrics.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-b\t0\tchr1\t5\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-a\t0\tchr1\t4\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=0",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not coordinate-sorted"));
}

#[test]
fn collectwgsmetrics_use_fast_algorithm_defaults_to_zero_sample_and_no_bq_histogram() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let fast_output = tempdir.path().join("wgs_fast.txt");
    let explicit_output = tempdir.path().join("wgs_sample0.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", fast_output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "USE_FAST_ALGORITHM=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", explicit_output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "SAMPLE_SIZE=0",
            "INCLUDE_BQ_HISTOGRAM=false",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&fast_output).expect("fast metrics exist"),
        fs::read_to_string(&explicit_output).expect("explicit metrics exist")
    );
}

#[test]
fn collectwgsmetrics_env_fast_default_matches_explicit_fast_algorithm() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let env_output = tempdir.path().join("wgs_env_fast.txt");
    let explicit_output = tempdir.path().join("wgs_explicit_fast.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_WGS_FAST_DEFAULT", "true")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", env_output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectWgsMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", explicit_output.display()),
            &format!("R={}", reference.display()),
            "COUNT_UNPAIRED=true",
            "USE_FAST_ALGORITHM=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&env_output).expect("env metrics exist"),
        fs::read_to_string(&explicit_output).expect("explicit metrics exist")
    );
}

#[test]
fn collectinsertsizemetrics_writes_metrics_histogram_and_chart() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## METRICS CLASS\tpicard.analysis.InsertSizeMetrics\n"));
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t\t\t\n"
    ));
    assert!(metrics.contains("insert_size\tAll_Reads.fr_count\n24\t1\n34\t1\n"));
    let histogram_pdf = fs::read_to_string(&histogram).expect("histogram PDF exists");
    assert!(histogram_pdf.starts_with("%PDF-1.4"));
    assert!(histogram_pdf.contains("/Count 1"));
    assert!(histogram_pdf.contains("CollectInsertSizeMetrics summary chart"));
}

#[test]
fn collectinsertsizemetrics_separates_pair_orientations() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "fr1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "fr1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "rf1\t83\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "rf1\t163\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
            "tan1\t67\tchr1\t200\t60\t4M\t=\t230\t34\tCCCC\tFFFF\n",
            "tan1\t131\tchr1\t230\t60\t4M\t=\t200\t-34\tGGGG\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("24\t24\t0\t24\t24\t24\t?\t1\tFR"));
    assert!(metrics.contains("34\t34\t0\t34\t34\t34\t?\t1\tRF"));
    assert!(metrics.contains("34\t34\t0\t34\t34\t34\t?\t1\tTANDEM"));
    assert!(metrics.contains(
        "insert_size\tAll_Reads.fr_count\tAll_Reads.rf_count\tAll_Reads.tandem_count\n24\t1\t0\t0\n34\t0\t1\t1\n"
    ));
}

#[test]
fn collectinsertsizemetrics_can_include_duplicate_pairs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "dup1\t1123\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "dup1\t1171\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "INCLUDE_DUPLICATES=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t\t\t\n"
    ));
    assert!(metrics.contains("insert_size\tAll_Reads.fr_count\n24\t1\n34\t1\n"));
}

#[test]
fn collectinsertsizemetrics_accepts_minimum_pct_short_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "M=0.5",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("insert_size\tAll_Reads.fr_count\n24\t1\n34\t1\n"));
}

#[test]
fn collectinsertsizemetrics_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("insert_size\tAll_Reads.fr_count\n24\t1\n34\t1\n"));
}

#[test]
fn collectinsertsizemetrics_honors_stop_after_with_documented_runtime_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "STOP_AFTER=2",
            "AS=true",
            "DEVIATIONS=5",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("24\t24\t0\t24\t24\t24\t?\t1\tFR"));
    assert!(metrics.contains("insert_size\tAll_Reads.fr_count\n24\t1\n"));
    assert!(!metrics.contains("34\t1\n"));
    assert!(histogram.metadata().expect("histogram exists").len() > 0);
}

#[test]
fn collectinsertsizemetrics_can_accumulate_by_sample() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tRG:Z:rg1\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\tRG:Z:rg1\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "METRIC_ACCUMULATION_LEVEL=SAMPLE",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t\t\t\n",
    ));
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\tsampleA\t\t\n",
    ));
    assert!(
        metrics.contains("insert_size\tAll_Reads.fr_count\tsampleA.fr_count\n24\t1\t1\n34\t1\t1\n")
    );
}

#[test]
fn collectinsertsizemetrics_can_accumulate_by_library() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tRG:Z:rg1\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\tRG:Z:rg1\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "METRIC_ACCUMULATION_LEVEL=LIBRARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\tsampleA\tlib1\t\n",
    ));
    assert!(
        metrics.contains("insert_size\tAll_Reads.fr_count\tlib1.fr_count\n24\t1\t1\n34\t1\t1\n")
    );
}

#[test]
fn collectinsertsizemetrics_can_accumulate_by_read_group() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("insert_size_metrics.txt");
    let histogram = tempdir.path().join("insert_size_histogram.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tRG:Z:rg1\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\tRG:Z:rg1\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectInsertSizeMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", histogram.display()),
            "METRIC_ACCUMULATION_LEVEL=READ_GROUP",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\tsampleA\tlib1\tunit1\n",
    ));
    assert!(
        metrics.contains("insert_size\tAll_Reads.fr_count\tunit1.fr_count\n24\t1\t1\n34\t1\t1\n")
    );
}

#[test]
fn collectgcbiasmetrics_can_also_emit_unique_duplicate_filtered_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let detail = tempdir.path().join("gc_bias.detail.txt");
    let summary = tempdir.path().join("gc_bias.summary.txt");
    let chart = tempdir.path().join("gc_bias.pdf");

    fs::write(
        &reference,
        concat!(
            ">low\n",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            ">high\n",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n",
        ),
    )
    .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:low\tLN:40\n",
            "@SQ\tSN:high\tLN:40\n",
            "low1\t0\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\tFFFFFFFFFFFFFFFFFFFF\n",
            "lowdup\t1024\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\t!!!!!!!!!!!!!!!!!!!!\n",
            "high1\t0\thigh\t1\t60\t20M\t*\t0\t0\tCCCCCCCCCCCCCCCCCCCC\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectGcBiasMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", detail.display()),
            &format!("S={}", summary.display()),
            &format!("CHART={}", chart.display()),
            &format!("R={}", reference.display()),
            "SCAN_WINDOW_SIZE=20",
            "MINIMUM_GENOME_FRACTION=0",
            "ALSO_IGNORE_DUPLICATES=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let detail_text = fs::read_to_string(&detail).expect("detail output exists");
    assert!(detail_text.contains("All Reads\tALL\t0\t19\t2\t18.5\t1.333333"));
    assert!(detail_text.contains("All Reads\tUNIQUE\t0\t19\t1\t37\t1"));
    assert!(detail_text.contains("All Reads\tUNIQUE\t100\t19\t1\t37\t1"));

    let summary_text = fs::read_to_string(&summary).expect("summary output exists");
    assert!(summary_text.contains("All Reads\tALL\t20\t3\t3\t0\t16.666667"));
    assert!(summary_text.contains("All Reads\tUNIQUE\t20\t2\t2\t0\t0"));
}

#[test]
fn collectmultiplemetrics_runs_supported_programs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple");
    fs::write(
        &input,
        "\
@HD\tVN:1.6\tSO:coordinate
@SQ\tSN:chr1\tLN:1000
pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF
pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF
pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF
pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF
",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CollectMultipleMetrics",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "PROGRAM=null",
        "PROGRAM=CollectAlignmentSummaryMetrics",
        "PROGRAM=CollectInsertSizeMetrics",
        "PROGRAM=QualityScoreDistribution",
        "PROGRAM=MeanQualityByCycle",
        "PROGRAM=CollectQualityYieldMetrics",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    assert!(output.with_extension("alignment_summary_metrics").exists());
    assert!(output.with_extension("read_length_histogram.pdf").exists());
    assert!(output.with_extension("insert_size_metrics").exists());
    assert!(output.with_extension("insert_size_histogram.pdf").exists());
    assert!(
        output
            .with_extension("quality_distribution_metrics")
            .exists()
    );
    assert!(output.with_extension("quality_distribution.pdf").exists());
    assert!(output.with_extension("quality_by_cycle_metrics").exists());
    assert!(output.with_extension("quality_by_cycle.pdf").exists());
    assert!(output.with_extension("quality_yield_metrics").exists());
}

#[test]
fn collectmultiplemetrics_accepts_use_fast_algorithm_with_wgs_program() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.bam");
    let sam = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("multiple_wgs_fast");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t3\t20\t4M\t*\t0\t0\tGTAC\tFFFF\n",
            "read-c\t0\tchr1\t9\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", sam.display()),
            &format!("O={}", input.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=CollectAlignmentSummaryMetrics",
            "PROGRAM=CollectWgsMetrics",
            &format!("REFERENCE_SEQUENCE={}", reference.display()),
            "USE_FAST_ALGORITHM=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("alignment_summary_metrics").exists());
    assert!(output.with_extension("wgs_metrics").exists());
}

#[test]
fn collectmultiplemetrics_appends_file_extension_to_metric_outputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectQualityYieldMetrics",
            "EXT=.txt",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("quality_yield_metrics.txt").exists());
    assert!(!output.with_extension("quality_yield_metrics").exists());
}

#[test]
fn collectmultiplemetrics_accepts_common_runtime_sidecar_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_runtime");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectQualityYieldMetrics",
            "CREATE_INDEX=true",
            "CREATE_MD5_FILE=true",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "COMPRESSION_LEVEL=5",
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("quality_yield_metrics").exists());
    assert!(!output.with_extension("quality_yield_metrics.md5").exists());
    assert!(!output.with_extension("quality_yield_metrics.idx").exists());
}

#[test]
fn collectmultiplemetrics_runs_picard_default_programs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_default");
    fs::write(
        &input,
        "\
@HD\tVN:1.6\tSO:coordinate
@SQ\tSN:chr1\tLN:1000
pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF
pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF
",
    )
    .unwrap();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("alignment_summary_metrics").exists());
    assert!(output.with_extension("read_length_histogram.pdf").exists());
    assert!(
        output
            .with_extension("base_distribution_by_cycle_metrics")
            .exists()
    );
    assert!(
        output
            .with_extension("base_distribution_by_cycle.pdf")
            .exists()
    );
    assert!(output.with_extension("insert_size_metrics").exists());
    assert!(output.with_extension("insert_size_histogram.pdf").exists());
    assert!(output.with_extension("quality_by_cycle_metrics").exists());
    assert!(output.with_extension("quality_by_cycle.pdf").exists());
    assert!(
        output
            .with_extension("quality_distribution_metrics")
            .exists()
    );
    assert!(output.with_extension("quality_distribution.pdf").exists());
    assert!(!output.with_extension("quality_yield_metrics").exists());
}

#[test]
fn collectmultiplemetrics_runs_explicit_gc_bias_program() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("multiple_gc");
    fs::write(
        &reference,
        concat!(
            ">low\n",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            ">high\n",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n",
        ),
    )
    .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:low\tLN:40\n",
            "@SQ\tSN:high\tLN:40\n",
            "low1\t0\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\tFFFFFFFFFFFFFFFFFFFF\n",
            "high1\t0\thigh\t1\t60\t20M\t*\t0\t0\tCCCCCCCCCCCCCCCCCCCC\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "PROGRAM=null",
            "PROGRAM=CollectGcBiasMetrics",
            "EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=20",
            "EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=0",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("gc_bias.detail_metrics").exists());
    assert!(output.with_extension("gc_bias.summary_metrics").exists());
    assert!(output.with_extension("gc_bias.pdf").exists());
}

#[test]
fn collectmultiplemetrics_forwards_gc_bias_duplicate_extra_argument() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("multiple_gc_duplicates");
    fs::write(
        &reference,
        concat!(
            ">low\n",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            ">high\n",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\n",
        ),
    )
    .expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:low\tLN:40\n",
            "@SQ\tSN:high\tLN:40\n",
            "low1\t0\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\tFFFFFFFFFFFFFFFFFFFF\n",
            "lowdup\t1024\tlow\t1\t60\t20M\t*\t0\t0\tAAAAAAAAAAAAAAAAAAAA\t!!!!!!!!!!!!!!!!!!!!\n",
            "high1\t0\thigh\t1\t60\t20M\t*\t0\t0\tCCCCCCCCCCCCCCCCCCCC\tFFFFFFFFFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "PROGRAM=null",
            "PROGRAM=CollectGcBiasMetrics",
            "EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=20",
            "EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=0",
            "EXTRA_ARGUMENT=CollectGcBiasMetrics::ALSO_IGNORE_DUPLICATES=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let detail = fs::read_to_string(output.with_extension("gc_bias.detail_metrics"))
        .expect("gc bias detail metrics exist");
    assert!(detail.contains("All Reads\tUNIQUE\t0\t19\t1\t37\t1"));
    assert!(detail.contains("All Reads\tUNIQUE\t100\t19\t1\t37\t1"));
}

#[test]
fn collectmultiplemetrics_forwards_quality_distribution_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_quality");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t0\t*\t*\t0\t0\tTGCA\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=QualityScoreDistribution",
            "EXTRA_ARGUMENT=QualityScoreDistribution::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("quality_distribution_metrics"))
        .expect("quality distribution metrics exist");
    assert!(metrics.contains("QUALITY\tCOUNT_OF_Q\n37\t4\n"));
    assert!(!metrics.contains("0\t4\n"));
}

#[test]
fn collectmultiplemetrics_forwards_mean_quality_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_mean_quality");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t0\t*\t*\t0\t0\tTGCA\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=MeanQualityByCycle",
            "EXTRA_ARGUMENT=MeanQualityByCycle::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("quality_by_cycle_metrics"))
        .expect("mean quality by cycle metrics exist");
    assert!(metrics.contains("CYCLE\tMEAN_QUALITY\n1\t37\n2\t37\n3\t37\n4\t37\n"));
}

#[test]
fn collectmultiplemetrics_forwards_quality_yield_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_yield");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "secondary\t256\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "supplemental\t2048\tchr1\t3\t60\t4M\t*\t0\t0\tACGT\tEEEE\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectQualityYieldMetrics",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SECONDARY_ALIGNMENTS=true",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("quality_yield_metrics"))
        .expect("quality yield metrics exist");
    assert!(metrics.contains("3\t3\t4\t12\t12\t12\t12\t12\t12\t22\t22\n"));
}

#[test]
fn collectmultiplemetrics_forwards_quality_yield_use_original_qualities_argument() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_yield_original");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectQualityYieldMetrics",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::USE_ORIGINAL_QUALITIES=false",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("quality_yield_metrics"))
        .expect("quality yield metrics exist");
    assert!(metrics.contains("1\t1\t4\t4\t4\t0\t0\t0\t0\t0\t0\n"));
}

#[test]
fn collectmultiplemetrics_threads_forwards_quality_yield_use_original_qualities_argument() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let bam = tempdir.path().join("input.bam");
    let output = tempdir.path().join("multiple_yield_threads");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\n",
            "secondary\t256\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "supplemental\t2048\tchr1\t3\t60\t4M\t*\t0\t0\tACGT\tEEEE\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", bam.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_CMM_THREADS", "2")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", bam.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectAlignmentSummaryMetrics",
            "PROGRAM=CollectQualityYieldMetrics",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::USE_ORIGINAL_QUALITIES=false",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SECONDARY_ALIGNMENTS=true",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("quality_yield_metrics"))
        .expect("quality yield metrics exist");
    assert!(metrics.contains("3\t3\t4\t12\t12\t8\t8\t8\t8\t14\t14\n"));
    assert!(output.with_extension("alignment_summary_metrics").exists());
}

#[test]
fn collectmultiplemetrics_forwards_base_distribution_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_base");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t0\t4M\t*\t0\t0\tNNNN\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectBaseDistributionByCycle",
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("base_distribution_by_cycle_metrics"))
        .expect("base distribution metrics exist");
    assert!(metrics.contains("1\t1\t100\t0\t0\t0\t0\n"));
}

#[test]
fn collectmultiplemetrics_threads_forwards_base_distribution_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let bam = tempdir.path().join("input.bam");
    let output = tempdir.path().join("multiple_base_threads");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", bam.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_CMM_THREADS", "2")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", bam.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectAlignmentSummaryMetrics",
            "PROGRAM=CollectBaseDistributionByCycle",
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("base_distribution_by_cycle_metrics"))
        .expect("base distribution metrics exist");
    assert!(metrics.contains("1\t1\t100\t0\t0\t0\t0\n"));
}

#[test]
fn collectmultiplemetrics_threads_runs_alignment_and_insert_size_together() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let bam = tempdir.path().join("input.bam");
    let output = tempdir.path().join("multiple_alignment_insert_threads");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "primary\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "secondary\t355\tchr1\t50\t60\t4M\t=\t70\t20\tAAAA\tFFFF\n",
            "supplemental\t827\tchr1\t60\t60\t4M\t=\t10\t-50\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", bam.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_CMM_THREADS", "2")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", bam.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectAlignmentSummaryMetrics",
            "PROGRAM=CollectInsertSizeMetrics",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let alignment_metrics = fs::read_to_string(output.with_extension("alignment_summary_metrics"))
        .expect("alignment summary metrics exist");
    let insert_metrics = fs::read_to_string(output.with_extension("insert_size_metrics"))
        .expect("insert-size metrics exist");

    assert!(alignment_metrics.contains("PAIR\t"));
    assert!(
        alignment_metrics
            .contains("READ_LENGTH\tPAIRED_TOTAL_LENGTH_COUNT\tPAIRED_ALIGNED_LENGTH_COUNT")
    );
    assert!(insert_metrics.contains("\tFR\t"));
    assert!(insert_metrics.contains("24\t1\n"));
}

#[test]
fn collectmultiplemetrics_threads_runs_quality_yield_with_base_distribution() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let bam = tempdir.path().join("input.bam");
    let output = tempdir.path().join("multiple_quality_yield_threads");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "primary\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\t!!!!\tOQ:Z:FFFF\n",
            "primary\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\t!!!!\tOQ:Z:FFFF\n",
            "secondary\t355\tchr1\t50\t60\t4M\t=\t70\t20\tAAAA\tFFFF\tOQ:Z:BBBB\n",
            "supplemental\t827\tchr1\t60\t60\t4M\t=\t10\t-50\tTTTT\tEEEE\tOQ:Z:CCCC\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", bam.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_CMM_THREADS", "2")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", bam.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectQualityYieldMetrics",
            "PROGRAM=CollectBaseDistributionByCycle",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SECONDARY_ALIGNMENTS=true",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true",
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::USE_ORIGINAL_QUALITIES=true",
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let quality_metrics = fs::read_to_string(output.with_extension("quality_yield_metrics"))
        .expect("quality yield metrics exist");
    assert!(quality_metrics.contains(
        "## METRICS CLASS\tpicard.analysis.CollectQualityYieldMetrics$QualityYieldMetrics"
    ));
    let base_metrics =
        fs::read_to_string(output.with_extension("base_distribution_by_cycle_metrics"))
            .expect("base distribution metrics exist");
    assert!(base_metrics.contains("1\t1\t100\t0\t0\t0\t0"));
}

#[test]
fn collectmultiplemetrics_threads_respects_conflicting_quality_filters() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let bam = tempdir.path().join("input.bam");
    let output = tempdir.path().join("multiple_quality_filter_threads");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "unmapped\t4\t*\t0\t0\t4M\t*\t0\t0\tTTTT\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input.display()),
            &format!("O={}", bam.display()),
            "SORT_ORDER=coordinate",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_CMM_THREADS", "2")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", bam.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectBaseDistributionByCycle",
            "PROGRAM=QualityScoreDistribution",
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::ALIGNED_READS_ONLY=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let base_metrics =
        fs::read_to_string(output.with_extension("base_distribution_by_cycle_metrics"))
            .expect("base distribution metrics exist");
    assert!(base_metrics.contains("1	1	100	0	0	0	0"));

    let quality_metrics = fs::read_to_string(output.with_extension("quality_distribution_metrics"))
        .expect("quality distribution metrics exist");
    assert!(quality_metrics.contains("0	4"));
    assert!(quality_metrics.contains("37	4"));
}
#[test]
fn collectmultiplemetrics_forwards_insert_size_extra_arguments() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_insert");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\n",
            "pairdup\t1123\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\n",
            "pairdup\t1171\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectInsertSizeMetrics",
            "EXTRA_ARGUMENT=CollectInsertSizeMetrics::INCLUDE_DUPLICATES=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("insert_size_metrics"))
        .expect("insert size metrics exist");
    assert!(metrics.contains("\t2\tFR\t"));
    assert!(metrics.contains("24\t1\n"));
    assert!(metrics.contains("34\t1\n"));
}

#[test]
fn collectmultiplemetrics_forwards_insert_size_accumulation_level() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_insert_read_group");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tRG:Z:rg1\n",
            "pair2\t99\tchr1\t100\t60\t4M\t=\t130\t34\tAAAA\tFFFF\tRG:Z:rg1\n",
            "pair2\t147\tchr1\t130\t60\t4M\t=\t100\t-34\tTTTT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectInsertSizeMetrics",
            "METRIC_ACCUMULATION_LEVEL=READ_GROUP",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("insert_size_metrics"))
        .expect("insert size metrics exist");
    assert!(metrics.contains(
        "29\t24\t5\t24\t34\t29\t7.071068\t2\tFR\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\t11\tsampleA\tlib1\tunit1\n",
    ));
    assert!(
        metrics.contains("insert_size\tAll_Reads.fr_count\tunit1.fr_count\n24\t1\t1\n34\t1\t1\n")
    );
}

#[test]
fn collectmultiplemetrics_forwards_alignment_accumulation_level() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("multiple_alignment_read_group");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sampleA\tLB:lib1\tPL:ILLUMINA\tPU:unit1\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\tRG:Z:rg1\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "CollectMultipleMetrics",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "PROGRAM=null",
            "PROGRAM=CollectAlignmentSummaryMetrics",
            "METRIC_ACCUMULATION_LEVEL=READ_GROUP",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(output.with_extension("alignment_summary_metrics"))
        .expect("alignment summary metrics exist");
    assert!(metrics.contains(
        "UNPAIRED\t2\t2\t1\t0\t1\t0.5\t4\t1\t4\t4\t0\t0\t0\t0\t4\t0\t4\t0\t4\t4\t2\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\tsampleA\tlib1\tunit1\n",
    ));
}

#[test]
fn fixmateinformation_updates_pair_fields_and_tags() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
            "single\t0\tchr1\t50\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(
        fixed.contains("pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
    assert!(
        fixed.contains("pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
    assert!(fixed.contains("single\t0\tchr1\t50\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n"));
}

#[test]
fn fixmateinformation_accepts_multiple_inputs_with_split_pair() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.sam");
    let second = tempdir.path().join("second.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &first,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("first input fixture is written");
    fs::write(
        &second,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("second input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(
        fixed.contains("pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
    assert!(
        fixed.contains("pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
}

#[test]
fn fixmateinformation_honors_mate_cigar_short_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tMC:Z:stale\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\tMC:Z:stale\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "AS=true",
            "SORT_ORDER=queryname",
            "MC=false",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(fixed.contains("pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tMQ:i:60\n"));
    assert!(fixed.contains("pair1\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\tFFFF\tMQ:i:60\n"));
    assert!(!fixed.contains("MC:Z:"));
}

#[test]
fn fixmateinformation_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(
        fixed.contains("pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
}

#[test]
fn fixmateinformation_writes_md5_sidecar_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    let md5_path = tempdir.path().join("fixed.sam.md5");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            "CREATE_MD5_FILE=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output_bytes = fs::read(&output).expect("fixed SAM exists");
    let md5 = fs::read_to_string(&md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5, format!("{:x}", md5::compute(output_bytes)));
}

#[test]
fn fixmateinformation_can_coordinate_sort_and_index_bam() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output_bam = tempdir.path().join("fixed.bam");
    let output_sam = tempdir.path().join("fixed.sam");
    let bai_path = tempdir.path().join("fixed.bai");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair-b\t99\tchr1\t50\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair-b\t147\tchr1\t70\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
            "pair-a\t99\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "pair-a\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output_bam.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=coordinate",
            "CREATE_INDEX=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();
    assert!(bai_path.exists());

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", output_bam.display()),
            &format!("O={}", output_sam.display()),
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output_sam).expect("fixed SAM exists");
    assert!(fixed.contains("@HD\tVN:1.6\tSO:coordinate"));
    assert_eq!(
        record_names(&fixed),
        vec!["pair-a", "pair-a", "pair-b", "pair-b"]
    );
}

#[test]
fn fixmateinformation_coordinate_output_uses_bounded_temp_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    let sort_tmp = tempdir.path().join("fixmate-tmp");
    fs::create_dir(&sort_tmp).expect("sort tmp exists");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair-c\t99\tchr1\t80\t60\t4M\t*\t0\t0\tCCCC\tFFFF\n",
            "pair-c\t147\tchr1\t90\t60\t4M\t*\t0\t0\tGGGG\tFFFF\n",
            "pair-a\t99\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "pair-a\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
            "pair-b\t99\tchr1\t50\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair-b\t147\tchr1\t70\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=coordinate",
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert_eq!(
        record_names(&fixed),
        vec!["pair-a", "pair-a", "pair-b", "pair-b", "pair-c", "pair-c"]
    );
    assert!(
        fixed.contains("pair-a\t99\tchr1\t10\t60\t4M\t=\t30\t24\tAAAA\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("fixmate tmp readable")
            .next()
            .is_none(),
        "FixMateInformation coordinate output should clean temporary runs"
    );
}

#[test]
fn fixmateinformation_can_write_unsorted_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=unsorted",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(fixed.contains("@HD\tVN:1.6\tSO:unsorted"));
    assert!(
        fixed.contains("pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
}

#[test]
fn fixmateinformation_requires_upstream_picard_for_coordinate_input_without_assume_sorted() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair-b\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair-a\t99\tchr1\t20\t60\t4M\t*\t0\t0\tAAAA\tFFFF\n",
            "pair-b\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
            "pair-a\t147\tchr1\t40\t60\t4M\t*\t0\t0\tTTTT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .env("TURBO_PICARD_DISABLE_AUTO_FALLBACK", "1")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "FixMateInformation non-queryname input should use upstream Picard",
        ));
}

#[test]
fn fixmateinformation_updates_supplementary_records_to_primary_mate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "pair1\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "pair1\t2147\tchr1\t100\t60\t4M\t*\t0\t0\tGGGG\tFFFF\n",
            "pair1\t147\tchr1\t30\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let fixed = fs::read_to_string(&output).expect("fixed SAM exists");
    assert!(
        fixed.contains("pair1\t2147\tchr1\t100\t60\t4M\t=\t30\t24\tGGGG\tFFFF\tMC:Z:4M\tMQ:i:60\n")
    );
}

#[test]
fn fixmateinformation_rejects_missing_mate_when_not_ignored() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("fixed.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "single\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "FixMateInformation",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ASSUME_SORTED=true",
            "SORT_ORDER=queryname",
            "IGNORE_MISSING_MATES=false",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Missing second read of pair: single",
        ));
}

#[test]
fn intervallisttools_concats_sorts_and_uniques_intervals() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.interval_list");
    let second = tempdir.path().join("second.interval_list");
    let output = tempdir.path().join("merged.interval_list");
    fs::write(
        &first,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t1\t10\t+\ta\n",
            "chr1\t11\t20\t+\tb\n",
            "chr1\t30\t40\t+\tc\n",
        ),
    )
    .expect("first interval list is written");
    fs::write(
        &second,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t5\t15\t+\td\n",
        ),
    )
    .expect("second interval list is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "IntervalListTools",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "ACTION=CONCAT",
            "SORT=true",
            "UNIQUE=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval output exists"),
        concat!(
            "@HD\tVN:1.6\tSO:unsorted\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t1\t20\t+\ta|d|b\n",
            "chr1\t30\t40\t+\tc\n",
        )
    );
}

#[test]
fn intervallisttools_can_keep_abutting_intervals_separate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.interval_list");
    let output = tempdir.path().join("merged.interval_list");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t1\t10\t+\ta\n",
            "chr1\t11\t20\t+\tb\n",
            "chr1\t21\t25\t+\tc\n",
            "chr1\t30\t40\t+\td\n",
            "chr1\t35\t45\t+\te\n",
        ),
    )
    .expect("interval list is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "IntervalListTools",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ACTION=CONCAT",
            "SORT=true",
            "UNIQUE=true",
            "DONT_MERGE_ABUTTING=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval output exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t1\t10\t+\ta\n",
            "chr1\t11\t20\t+\tb\n",
            "chr1\t21\t25\t+\tc\n",
            "chr1\t30\t45\t+\td|e\n",
        )
    );
}

#[test]
fn intervallisttools_applies_positive_padding_and_clamps_to_dictionary() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.interval_list");
    let output = tempdir.path().join("padded.interval_list");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "chr1\t3\t5\t+\tnear-start\n",
            "chr1\t95\t98\t+\tnear-end\n",
        ),
    )
    .expect("interval list is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "IntervalListTools",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ACTION=CONCAT",
            "SORT=true",
            "UNIQUE=false",
            "PADDING=10",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval output exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "chr1\t1\t15\t+\tnear-start\n",
            "chr1\t85\t100\t+\tnear-end\n",
        )
    );
}

#[test]
fn revertsam_restores_original_qualities_and_unmaps_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n",
            "pair1\t1123\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\t!!!!\tRG:Z:rg1\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\tPG:Z:align\tMC:Z:4M\tMQ:i:60\n",
            "pair1\t1171\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\t!!!!\tRG:Z:rg1\tOQ:Z:EEEE\tNM:i:0\tMD:Z:4\tPG:Z:align\tMC:Z:4M\tMQ:i:60\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n",
            "pair1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
            "pair1\t141\t*\t0\t0\t*\t*\t0\t0\tTGCA\tEEEE\tRG:Z:rg1\n",
        )
    );
}

#[test]
fn revertsam_clears_custom_requested_attributes() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n",
            "read1\t1024\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t!!!!\tRG:Z:rg1\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\tXT:Z:clearme\tXA:i:7\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ATTRIBUTE_TO_CLEAR=XT",
            "ATTRIBUTE_TO_CLEAR=XA",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n",
            "read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
        )
    );
}

#[test]
fn revertsam_reverses_negative_strand_sequence_and_requested_attributes() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read1\t16\tchr1\t10\t60\t4M\t*\t0\t0\tACGA\t!!!!\tOQ:Z:abcd\tXR:Z:wxyz\tXC:Z:ACGA\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "ATTRIBUTE_TO_REVERSE=XR",
            "ATTRIBUTE_TO_REVERSE_COMPLEMENT=XC",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read1\t4\t*\t0\t0\t*\t*\t0\t0\tTCGT\tdcba\tXC:Z:TCGT\tXR:Z:zyxw\n",
        )
    );
}

#[test]
fn revertsam_restores_hardclips_from_xb_and_xq_tags() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read1\t16\tchr1\t10\t60\t2H4M3H\t*\t0\t0\tACGA\t!!!!\tOQ:Z:abcd\tXB:Z:TTCCC\tXQ:Z:vwxyz\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read1\t4\t*\t0\t0\t*\t*\t0\t0\tTCGTTTCCC\tdcbavwxyz\n",
        )
    );
}

#[test]
fn revertsam_writes_md5_sidecar_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    let md5_path = tempdir.path().join("reverted.sam.md5");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n",
            "pair1\t1123\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\t!!!!\tRG:Z:rg1\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\tPG:Z:align\tMC:Z:4M\tMQ:i:60\n",
            "pair1\t1171\tchr1\t30\t60\t4M\t=\t10\t-24\tTGCA\t!!!!\tRG:Z:rg1\tOQ:Z:EEEE\tNM:i:0\tMD:Z:4\tPG:Z:align\tMC:Z:4M\tMQ:i:60\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "CREATE_MD5_FILE=true",
            "COMPRESSION_LEVEL=5",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output_bytes = fs::read(&output).expect("reverted SAM exists");
    let md5 = fs::read_to_string(&md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5, format!("{:x}", md5::compute(output_bytes)));
}

#[test]
fn revertsam_accepts_create_index_without_writing_bai() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.bam");
    let bai_path = tempdir.path().join("reverted.bai");
    let output_sam = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read1\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\t!!!!\tOQ:Z:HHHH\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "CREATE_INDEX=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();
    assert!(!bai_path.exists());

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", output.display()),
            &format!("O={}", output_sam.display()),
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output_sam).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read1\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tHHHH\n",
        )
    );
}

#[test]
fn revertsam_writes_index_for_coordinate_output_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.bam");
    let bai_path = tempdir.path().join("reverted.bai");
    let output_sam = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "SORT_ORDER=coordinate",
            "CREATE_INDEX=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();
    assert!(bai_path.exists());

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", output.display()),
            &format!("O={}", output_sam.display()),
        ])
        .assert()
        .success();

    let output_text = fs::read_to_string(output_sam).expect("reverted SAM exists");
    assert!(output_text.contains("@HD\tVN:1.6\tSO:coordinate\n"));
    assert!(output_text.contains("read-a\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\n"));
}

#[test]
fn revertsam_accepts_non_queryname_sort_order_outputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-b\t0\tchr1\t50\t60\t4M\t*\t0\t0\tCCCC\tFFFF\tOQ:Z:HHHH\n",
            "read-b\t1024\tchr1\t70\t60\t4M\t*\t0\t0\tGGGG\tFFFF\tOQ:Z:IIII\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\tFFFF\tOQ:Z:JJJJ\n",
        ),
    )
    .expect("input fixture is written");

    for sort_order in ["unsorted", "coordinate"] {
        let output = tempdir.path().join(format!("{sort_order}.sam"));
        Command::cargo_bin("picard")
            .expect("binary exists")
            .args([
                "RevertSam",
                &format!("I={}", input.display()),
                &format!("O={}", output.display()),
                &format!("SORT_ORDER={sort_order}"),
                "VALIDATION_STRINGENCY=SILENT",
                "QUIET=true",
            ])
            .assert()
            .success();

        let reverted = fs::read_to_string(output).expect("reverted SAM exists");
        assert!(reverted.contains(&format!("@HD\tVN:1.6\tSO:{sort_order}\n")));
        assert_eq!(record_names(&reverted), vec!["read-b", "read-b", "read-a"]);
    }
}

#[test]
fn revertsam_queryname_output_uses_bounded_temp_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    let sort_tmp = tempdir.path().join("revertsam-tmp");
    fs::create_dir(&sort_tmp).expect("sort tmp exists");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-c\t0\tchr1\t70\t60\t4M\t*\t0\t0\tCCCC\t!!!!\tOQ:Z:HHHH\tNM:i:0\tMD:Z:4\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\t!!!!\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\n",
            "read-b\t0\tchr1\t40\t60\t4M\t*\t0\t0\tGGGG\t!!!!\tOQ:Z:IIII\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "COMPRESSION_LEVEL=5",
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let reverted = fs::read_to_string(&output).expect("reverted SAM exists");
    assert_eq!(record_names(&reverted), vec!["read-a", "read-b", "read-c"]);
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("revertsam tmp readable")
            .next()
            .is_none(),
        "RevertSam queryname output should clean temporary runs"
    );
}

#[test]
fn revertsam_can_keep_alignment_information_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@PG\tID:aligner\tPN:aligner\n",
            "read1\t1024\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\tMC:Z:4M\tMQ:i:60\tXT:Z:clearme\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "REMOVE_ALIGNMENT_INFORMATION=false",
            "RESTORE_HARDCLIPS=false",
            "ATTRIBUTE_TO_CLEAR=XT",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@PG\tID:aligner\tPN:aligner\n",
            "read1\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\tMC:Z:4M\tMD:Z:4\tNM:i:0\tMQ:i:60\tXT:Z:clearme\n",
        )
    );
}

#[test]
fn revertsam_filters_secondary_and_supplementary_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("reverted.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-primary\t0\tchr1\t10\t60\t4M\t*\t0\t0\tAAAA\t!!!!\tOQ:Z:FFFF\tNM:i:0\tMD:Z:4\n",
            "read-secondary\t256\tchr1\t20\t60\t4M\t*\t0\t0\tCCCC\t!!!!\tOQ:Z:GGGG\tNM:i:0\tMD:Z:4\n",
            "read-supplementary\t2048\tchr1\t30\t60\t4M\t*\t0\t0\tGGGG\t!!!!\tOQ:Z:HHHH\tNM:i:0\tMD:Z:4\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "RevertSam",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("reverted SAM exists"),
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "read-primary\t4\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
        )
    );
}

#[test]
fn setnmmdanduqtags_computes_reference_tags() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("tagged.sam");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGA\tFFFF\n",
            "read2\t0\tchr1\t5\t60\t2M1I2M\t*\t0\t0\tACGTA\tFFFFF\n",
            "read3\t0\tchr1\t8\t60\t2M1D2M\t*\t0\t0\tTACG\tFFFF\n",
            "read4\t0\tchr1\t1\t60\t12M\t*\t0\t0\tACGTACGTACGT\tFFFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SetNmMdAndUqTags",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let tagged = fs::read_to_string(output).expect("tagged SAM exists");
    assert!(
        tagged.contains(
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGA\tFFFF\tMD:Z:3T0\tNM:i:1\tUQ:i:37\n"
        )
    );
    assert!(tagged.contains(
        "read2\t0\tchr1\t5\t60\t2M1I2M\t*\t0\t0\tACGTA\tFFFFF\tMD:Z:2G0T0\tNM:i:3\tUQ:i:74\n"
    ));
    assert!(tagged.contains(
        "read3\t0\tchr1\t8\t60\t2M1D2M\t*\t0\t0\tTACG\tFFFF\tMD:Z:2^C0G0T0\tNM:i:3\tUQ:i:74\n"
    ));
    assert!(tagged.contains(
        "read4\t0\tchr1\t1\t60\t12M\t*\t0\t0\tACGTACGTACGT\tFFFFFFFFFFFF\tMD:Z:12\tNM:i:0\tUQ:i:0\n"
    ));
}

#[test]
fn setnmmdanduqtags_set_only_uq_preserves_existing_nm_md() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("tagged.sam");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGA\tFFFF\tMD:Z:keep\tNM:i:99\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SetNmMdAndUqTags",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "SET_ONLY_UQ=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let tagged = fs::read_to_string(output).expect("tagged SAM exists");
    assert!(
        tagged.contains(
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGA\tFFFF\tMD:Z:keep\tNM:i:99\tUQ:i:37\n"
        )
    );
}

#[test]
fn setnmmdanduqtags_writes_md5_sidecar_but_no_index_for_sam_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("tagged.sam");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:12\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGA\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SetNmMdAndUqTags",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("R={}", reference.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(output.exists());
    assert!(tempdir.path().join("tagged.sam.md5").exists());
    assert!(!tempdir.path().join("tagged.sam.bai").exists());
    assert!(!tempdir.path().join("tagged.sam.idx").exists());
}

#[test]
fn validatesamfile_writes_summary_for_valid_and_warning_inputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let valid = tempdir.path().join("valid.sam");
    let warning = tempdir.path().join("warning.sam");
    let output = tempdir.path().join("summary.txt");
    fs::write(
        &valid,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\tNM:i:0\n",
        ),
    )
    .expect("valid fixture is written");
    fs::write(
        &warning,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("warning fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", valid.display()),
            &format!("O={}", output.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&output).expect("summary exists"),
        "No errors found\n"
    );

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", warning.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::str::contains("ERROR:MISSING_READ_GROUP\t1"))
        .stdout(predicate::str::contains("WARNING:MISSING_TAG_NM\t1"))
        .stdout(predicate::str::contains(
            "WARNING:RECORD_MISSING_READ_GROUP\t1",
        ))
        .stderr(predicate::str::contains(
            "ValidateSamFile found validation issues",
        ));
}

#[test]
fn validatesamfile_ignores_requested_summary_error_types() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let missing_nm = tempdir.path().join("missing_nm.sam");
    fs::write(
        &missing_nm,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("missing NM fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", missing_nm.display()),
            "MODE=SUMMARY",
            "IGNORE=MISSING_TAG_NM",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success()
        .stdout("No errors found\n");
}

#[test]
fn validatesamfile_reports_and_ignores_missing_platform_value() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let missing_platform = tempdir.path().join("missing_platform.sam");
    fs::write(
        &missing_platform,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\tNM:i:0\n",
        ),
    )
    .expect("missing platform fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", missing_platform.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("ERROR:MISSING_PLATFORM_VALUE\t1"))
        .stderr(predicate::str::contains(
            "ValidateSamFile found validation issues",
        ));

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", missing_platform.display()),
            "MODE=SUMMARY",
            "IGNORE=MISSING_PLATFORM_VALUE",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success()
        .stdout("No errors found\n");
}

#[test]
fn validatesamfile_reports_unmapped_record_with_nonzero_mapq() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let invalid = tempdir.path().join("invalid_mapq.sam");
    fs::write(
        &invalid,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "read1\t4\t*\t0\t60\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
        ),
    )
    .expect("invalid MAPQ fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", invalid.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("ERROR:INVALID_MAPPING_QUALITY\t1"))
        .stderr(predicate::str::contains(
            "ValidateSamFile found validation issues",
        ));

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", invalid.display()),
            "MODE=SUMMARY",
            "IGNORE=INVALID_MAPPING_QUALITY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success()
        .stdout("No errors found\n");
}

#[test]
fn validatesamfile_accepts_valid_adjacent_paired_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let paired = tempdir.path().join("paired.sam");
    fs::write(
        &paired,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "pair1\t99\tchr1\t1\t60\t4M\t=\t11\t14\tACGT\tFFFF\tRG:Z:rg1\tNM:i:0\n",
            "pair1\t147\tchr1\t11\t60\t4M\t=\t1\t-14\tTGCA\tFFFF\tRG:Z:rg1\tNM:i:0\n",
        ),
    )
    .expect("paired fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", paired.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success()
        .stdout("No errors found\n");
}

#[test]
fn validatesamfile_reports_missing_mate_and_honors_skip_mate_validation() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let orphan = tempdir.path().join("orphan.sam");
    fs::write(
        &orphan,
        concat!(
            "@HD\tVN:1.6\tSO:queryname\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "orphan1\t99\tchr1\t1\t60\t4M\t=\t11\t14\tACGT\tFFFF\tRG:Z:rg1\tNM:i:0\n",
        ),
    )
    .expect("orphan paired-read fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", orphan.display()),
            "MODE=SUMMARY",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("ERROR:MATE_NOT_FOUND\t1"))
        .stderr(predicate::str::contains(
            "ValidateSamFile found validation issues",
        ));

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", orphan.display()),
            "MODE=SUMMARY",
            "SKIP_MATE_VALIDATION=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success()
        .stdout("No errors found\n");
}

#[test]
fn validatesamfile_accepts_common_runtime_sidecar_options_without_sidecars() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let valid = tempdir.path().join("valid.sam");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("summary.txt");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference is written");
    fs::write(
        &valid,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\tNM:i:0\n",
        ),
    )
    .expect("valid fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", valid.display()),
            &format!("O={}", output.display()),
            "MODE=SUMMARY",
            &format!("R={}", reference.display()),
            "CREATE_INDEX=true",
            "CREATE_MD5_FILE=true",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "COMPRESSION_LEVEL=5",
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&output).expect("summary exists"),
        "No errors found\n"
    );
    assert!(!tempdir.path().join("summary.txt.md5").exists());
    assert!(!tempdir.path().join("summary.txt.idx").exists());
}

#[test]
fn validatesamfile_writes_verbose_details_for_supported_issue_types() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let warning = tempdir.path().join("warning.sam");
    fs::write(
        &warning,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("warning fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", warning.display()),
            "MODE=VERBOSE",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
    .assert()
        .failure()
        .stdout(predicate::str::contains(
            "ERROR::MISSING_READ_GROUP:Read groups is empty\n",
        ))
        .stdout(predicate::str::contains(
            "WARNING::RECORD_MISSING_READ_GROUP:Read name read1, A record is missing a read group\n",
        ))
        .stdout(predicate::str::contains(
            "WARNING::MISSING_TAG_NM:Record 1, Read name read1, NM tag (nucleotide differences) is missing\n",
        ))
        .stderr(predicate::str::contains(
            "ValidateSamFile found validation issues",
        ));
}

#[test]
fn validatesamfile_honors_verbose_max_output_limit() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let warning = tempdir.path().join("warning.sam");
    fs::write(
        &warning,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:100\n",
            "read1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read2\t0\tchr1\t2\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("warning fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ValidateSamFile",
            &format!("I={}", warning.display()),
            "MODE=VERBOSE",
            "MAX_OUTPUT=2",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stdout(concat!(
            "ERROR::MISSING_READ_GROUP:Read groups is empty\n",
            "WARNING::RECORD_MISSING_READ_GROUP:Read name read1, A record is missing a read group\n",
            "Maximum output of [2] errors reached.\n",
        ));
}

#[test]
fn liftovervcf_lifts_positive_single_block_chain_and_writes_rejects() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let output = tempdir.path().join("lifted.vcf");
    let reject = tempdir.path().join("reject.vcf");
    let reference = tempdir.path().join("ref.fa");
    let dictionary = tempdir.path().join("ref.dict");
    let chain = tempdir.path().join("identity.chain");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=100>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t10\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t11\t.\tG\tT\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &reference,
        ">chr1\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
    )
    .expect("reference is written");
    fs::write(&dictionary, "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:100\n").expect("dict is written");
    fs::write(
        &chain,
        "chain 100 chr1 100 + 0 100 chr1 100 + 0 100 1\n100\n",
    )
    .expect("chain is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "LiftoverVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHAIN={}", chain.display()),
            &format!("REJECT={}", reject.display()),
            &format!("R={}", reference.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let lifted = fs::read_to_string(output).expect("lifted VCF exists");
    assert!(lifted.contains("##INFO=<ID=ReverseComplementedAlleles"));
    assert!(lifted.contains(&format!("##reference=file:{}", reference.display())));
    assert!(lifted.contains("chr1\t10\t.\tA\tC\t.\tPASS\t.\n"));
    assert!(!lifted.contains("chr1\t11\t.\tG\tT"));

    let rejected = fs::read_to_string(reject).expect("reject VCF exists");
    assert!(rejected.contains("##FILTER=<ID=MismatchedRefAllele"));
    assert!(rejected.contains(
        "chr1\t11\t.\tG\tT\t.\tMismatchedRefAllele\tAttemptedAlleles=G*->T;AttemptedLocus=chr1:11-11\n"
    ));
}

#[test]
fn liftovervcf_writes_index_for_lifted_vcf_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let output = tempdir.path().join("lifted.vcf");
    let reject = tempdir.path().join("reject.vcf");
    let reference = tempdir.path().join("ref.fa");
    let dictionary = tempdir.path().join("ref.dict");
    let chain = tempdir.path().join("identity.chain");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=100>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t10\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t11\t.\tG\tT\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &reference,
        ">chr1\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
    )
    .expect("reference is written");
    fs::write(&dictionary, "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:100\n").expect("dict is written");
    fs::write(
        &chain,
        "chain 100 chr1 100 + 0 100 chr1 100 + 0 100 1\n100\n",
    )
    .expect("chain is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "LiftoverVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHAIN={}", chain.display()),
            &format!("REJECT={}", reject.display()),
            &format!("R={}", reference.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
            "CREATE_INDEX=true",
            "CREATE_MD5_FILE=true",
            "MAX_RECORDS_IN_RAM=500",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
        ])
        .assert()
        .success();

    assert!(output.with_extension("vcf.idx").exists());
    assert!(!reject.with_extension("vcf.idx").exists());
    assert!(!output.with_extension("vcf.md5").exists());
    assert!(!reject.with_extension("vcf.md5").exists());
}

#[test]
fn liftovervcf_honors_tmp_dir_and_forced_external_runs_for_lifted_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let sort_tmp = tempdir.path().join("liftover-tmp");
    fs::create_dir(&sort_tmp).expect("liftover temp dir is created");
    let input = tempdir.path().join("input.vcf");
    let output = tempdir.path().join("lifted.vcf");
    let reject = tempdir.path().join("reject.vcf");
    let reference = tempdir.path().join("ref.fa");
    let dictionary = tempdir.path().join("ref.dict");
    let chain = tempdir.path().join("identity.chain");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=100>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t30\tlate\tA\tC\t.\tPASS\t.\n",
            "chr1\t10\tearly\tA\tG\t.\tPASS\t.\n",
            "chr1\t20\tmiddle\tA\tT\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &reference,
        ">chr1\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
    )
    .expect("reference is written");
    fs::write(&dictionary, "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:100\n").expect("dict is written");
    fs::write(
        &chain,
        "chain 100 chr1 100 + 0 100 chr1 100 + 0 100 1\n100\n",
    )
    .expect("chain is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "LiftoverVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHAIN={}", chain.display()),
            &format!("REJECT={}", reject.display()),
            &format!("R={}", reference.display()),
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let lifted = fs::read_to_string(output).expect("lifted VCF exists");
    assert_eq!(
        lifted
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec![
            "chr1\t10\tearly\tA\tG\t.\tPASS\t.",
            "chr1\t20\tmiddle\tA\tT\t.\tPASS\t.",
            "chr1\t30\tlate\tA\tC\t.\tPASS\t.",
        ]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("liftover temp readable")
            .next()
            .is_none(),
        "external liftover sort should clean temporary runs"
    );
}

#[test]
fn liftovervcf_delegates_reverse_chain_to_fallback() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let fallback = fallback_script(tempdir.path(), 0);
    let log = tempdir.path().join("fallback.args");
    let chain = tempdir.path().join("reverse.chain");
    fs::write(
        &chain,
        "chain 100 chr1 100 - 0 100 chr1 100 + 0 100 1\n100\n",
    )
    .expect("chain is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.env(
        "TURBO_PICARD_FALLBACK_COMMAND",
        fallback.display().to_string(),
    )
    .env("TURBO_PICARD_FALLBACK_LOG", log.display().to_string())
    .args([
        "LiftoverVcf",
        "I=in.vcf",
        "O=out.vcf",
        &format!("CHAIN={}", chain.display()),
        "REJECT=reject.vcf",
        "R=ref.fa",
    ])
    .assert()
    .success();

    let fallback_args = fs::read_to_string(log).expect("fallback log exists");
    assert!(fallback_args.contains("LiftoverVcf\n"));
    assert!(fallback_args.contains("I=in.vcf\n"));
}

#[test]
fn qualityscoredistribution_writes_histogram_and_chart() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_score_distribution.txt");
    let chart = tempdir.path().join("quality_score_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t512\tchr1\t20\t60\t4M\t*\t0\t0\tNNNN\t!!!!\n",
            "read-c\t256\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "QualityScoreDistribution",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## HISTOGRAM\tjava.lang.Byte\n"));
    assert!(metrics.contains("QUALITY\tCOUNT_OF_Q\n37\t4\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn qualityscoredistribution_writes_original_quality_histogram() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_score_distribution.txt");
    let chart = tempdir.path().join("quality_score_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:FFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "QualityScoreDistribution",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("QUALITY\tCOUNT_OF_Q\tCOUNT_OF_OQ\n"));
    assert!(metrics.contains("0\t4\t0\n"));
    assert!(metrics.contains("37\t0\t4\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn qualityscoredistribution_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_score_distribution.txt");
    let chart = tempdir.path().join("quality_score_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "QualityScoreDistribution",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("QUALITY\tCOUNT_OF_Q\n37\t4\n"));
}

#[test]
fn qualityscoredistribution_honors_stop_after_and_assume_sorted_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("quality_score_distribution.txt");
    let chart = tempdir.path().join("quality_score_distribution.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tACGT\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "QualityScoreDistribution",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "STOP_AFTER=1",
            "AS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("QUALITY\tCOUNT_OF_Q\n37\t4\n"));
    assert!(!metrics.contains("\n0\t4\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn meanqualitybycycle_writes_histogram_and_chart() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("mean_quality_by_cycle.txt");
    let chart = tempdir.path().join("mean_quality_by_cycle.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t512\tchr1\t20\t60\t4M\t*\t0\t0\tNNNN\t!!!!\n",
            "read-c\t256\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MeanQualityByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("## HISTOGRAM\tjava.lang.Integer\n"));
    assert!(metrics.contains("CYCLE\tMEAN_QUALITY\n"));
    assert!(metrics.contains("1\t18.5\n2\t18.5\n3\t18.5\n4\t18.5\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn meanqualitybycycle_accepts_common_temp_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("mean_quality_by_cycle.txt");
    let chart = tempdir.path().join("mean_quality_by_cycle.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MeanQualityByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            &format!("TMP_DIR={}", tempdir.path().display()),
            "MAX_RECORDS_IN_RAM=500",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CYCLE\tMEAN_QUALITY\n"));
}

#[test]
fn meanqualitybycycle_honors_stop_after_and_assume_sorted_alias() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("mean_quality_by_cycle.txt");
    let chart = tempdir.path().join("mean_quality_by_cycle.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tACGT\t!!!!\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MeanQualityByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "STOP_AFTER=1",
            "AS=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CYCLE\tMEAN_QUALITY\n1\t37\n2\t37\n3\t37\n4\t37\n"));
    assert!(!metrics.contains("18.5"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn meanqualitybycycle_uses_original_quality_cycles_when_present() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("mean_quality_by_cycle.txt");
    let chart = tempdir.path().join("mean_quality_by_cycle.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t16\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t!!!!\tOQ:Z:\"#$%\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MeanQualityByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CYCLE\tMEAN_ORIGINAL_QUALITY\n1\t4\n2\t3\n3\t2\n4\t1\n"));
    assert!(chart.metadata().expect("chart exists").len() > 0);
}

#[test]
fn meanqualitybycycle_reverses_reverse_strand_cycles() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("mean_quality_by_cycle.txt");
    let chart = tempdir.path().join("mean_quality_by_cycle.pdf");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t16\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\t\"#$%\n",
        ),
    )
    .expect("input fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MeanQualityByCycle",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("CHART={}", chart.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let metrics = fs::read_to_string(&output).expect("metrics output exists");
    assert!(metrics.contains("CYCLE\tMEAN_QUALITY\n1\t4\n2\t3\n3\t2\n4\t1\n"));
}

#[test]
fn createsequencedictionary_writes_picard_dict() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("ref.dict");
    fs::write(
        &reference,
        concat!(
            ">chr1 first chromosome\n",
            "ACGTACGT\n",
            ">chr2\n",
            "NNNN\n",
        ),
    )
    .expect("reference fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CreateSequenceDictionary",
        &format!("R={}", reference.display()),
        &format!("O={}", output.display()),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    let dictionary = fs::read_to_string(&output).expect("dictionary output exists");
    assert_eq!(
        dictionary,
        format!(
            "@HD\tVN:1.6\n\
             @SQ\tSN:chr1\tLN:8\tM5:cc0af3a4fedb18378b4b57b98068e69f\tUR:file://{}\n\
             @SQ\tSN:chr2\tLN:4\tM5:ef95bc05180af51bfd945e93b2bbba8e\tUR:file://{}\n",
            reference.display(),
            reference.display(),
        )
    );
}

#[test]
fn createsequencedictionary_derives_output_and_reads_gzip() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa.gz");
    {
        let file = fs::File::create(&reference).expect("gzip reference is created");
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        use std::io::Write;
        encoder
            .write_all(b">chr1 first chromosome\nACGTACGT\n>chr2\nNNNN\n")
            .expect("gzip reference is written");
        encoder.finish().expect("gzip reference is finished");
    }

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CreateSequenceDictionary",
        &format!("REFERENCE={}", reference.display()),
        "NUM_SEQUENCES=1",
        "AS=GRCh38",
        "SP=Human",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    let dictionary = tempdir.path().join("ref.dict");
    let text = fs::read_to_string(&dictionary).expect("derived dictionary output exists");
    assert!(text.contains("@SQ\tSN:chr1\tLN:8\tM5:cc0af3a4fedb18378b4b57b98068e69f\tUR:file://"));
    assert!(text.contains("\tAS:GRCh38\tSP:Human\n"));
    assert!(!text.contains("SN:chr2"));
}

#[test]
fn createsequencedictionary_writes_alternate_names() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let alt_names = tempdir.path().join("alt.tsv");
    let output = tempdir.path().join("ref.dict");
    fs::write(
        &reference,
        concat!(
            ">chr1 first chromosome\n",
            "ACGTACGT\n",
            ">chr2\n",
            "NNNN\n",
        ),
    )
    .expect("reference fixture is written");
    fs::write(
        &alt_names,
        concat!("chr1\t1\n", "chr1\tCM000663.2\n", "chr2\t2\n"),
    )
    .expect("alt names fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CreateSequenceDictionary",
        &format!("R={}", reference.display()),
        &format!("O={}", output.display()),
        &format!("ALT_NAMES={}", alt_names.display()),
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    let dictionary = fs::read_to_string(&output).expect("dictionary output exists");
    assert!(dictionary.contains("\tSN:chr1\t"));
    assert!(dictionary.contains("\tAN:1,CM000663.2\n"));
    assert!(dictionary.contains("\tSN:chr2\t"));
    assert!(dictionary.contains("\tAN:2\n"));
}

#[test]
fn createsequencedictionary_writes_md5_sidecar_but_no_index_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = tempdir.path().join("ref.fa");
    let output = tempdir.path().join("ref.dict");
    fs::write(&reference, ">chr1 first chromosome\nACGTACGT\n")
        .expect("reference fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "CreateSequenceDictionary",
        &format!("R={}", reference.display()),
        &format!("O={}", output.display()),
        "CREATE_MD5_FILE=true",
        "CREATE_INDEX=true",
        "MAX_RECORDS_IN_RAM=1000",
        "TMP_DIR=/tmp",
        "USE_JDK_DEFLATER=true",
        "USE_JDK_INFLATER=true",
        "COMPRESSION_LEVEL=1",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    assert!(output.exists());
    let md5 = fs::read_to_string(format!("{}.md5", output.display()))
        .expect("dictionary md5 sidecar exists");
    assert_eq!(md5.len(), 32);
    assert!(md5.chars().all(|char| char.is_ascii_hexdigit()));
    assert!(!tempdir.path().join("ref.dict.bai").exists());
    assert!(!tempdir.path().join("ref.dict.idx").exists());
}

#[test]
fn normalizefasta_wraps_sequence_lines() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.fa");
    let output = tempdir.path().join("normalized.fa");
    fs::write(
        &input,
        concat!(
            ">chr1 first chromosome\n",
            "ACGTAC\n",
            "GT\n",
            ">chr2\n",
            "NNNNN\n"
        ),
    )
    .expect("input FASTA is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "NormalizeFasta",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "LINE_LENGTH=4",
        "TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE=true",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(output).expect("normalized FASTA exists"),
        concat!(">chr1\n", "ACGT\n", "ACGT\n", ">chr2\n", "NNNN\n", "N\n")
    );
}

#[test]
fn normalizefasta_accepts_common_noop_sidecar_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.fa");
    let output = tempdir.path().join("normalized.fa");
    fs::write(&input, ">chr1 first chromosome\nACGTACGT\n").expect("input FASTA is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "NormalizeFasta",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        "LINE_LENGTH=4",
        "CREATE_MD5_FILE=true",
        "CREATE_INDEX=true",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(&output).expect("normalized FASTA exists"),
        concat!(">chr1 first chromosome\n", "ACGT\n", "ACGT\n")
    );
    assert!(!tempdir.path().join("normalized.fa.md5").exists());
    assert!(!tempdir.path().join("normalized.fa.fai").exists());
}

#[test]
fn bedtointervallist_converts_and_sorts_by_dictionary_order() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let dictionary = tempdir.path().join("ref.dict");
    let bed = tempdir.path().join("targets.bed");
    let output = tempdir.path().join("targets.interval_list");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
        ),
    )
    .expect("dictionary fixture is written");
    fs::write(
        &bed,
        concat!(
            "chr2\t5\t10\tsecond\t0\t-\n",
            "chr1\t0\t4\tfirst\t0\t+\n",
            "chr1\t0\t4\tfirst\t0\t+\n",
        ),
    )
    .expect("BED fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "BedToIntervalList",
        &format!("I={}", bed.display()),
        &format!("O={}", output.display()),
        &format!("SD={}", dictionary.display()),
        "UNIQUE=true",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
            "chr1\t1\t4\t+\tfirst\n",
            "chr2\t6\t10\t-\tsecond\n",
        )
    );
}

#[test]
fn bedtointervallist_sort_false_preserves_input_order_with_picard_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let dictionary = tempdir.path().join("ref.dict");
    let bed = tempdir.path().join("targets.bed");
    let output = tempdir.path().join("targets.interval_list");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
        ),
    )
    .expect("dictionary fixture is written");
    fs::write(
        &bed,
        concat!("chr2\t5\t10\tsecond\t0\t-\n", "chr1\t0\t4\tfirst\t0\t+\n",),
    )
    .expect("BED fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "BedToIntervalList",
        &format!("I={}", bed.display()),
        &format!("O={}", output.display()),
        &format!("SD={}", dictionary.display()),
        "SORT=false",
        "UNIQUE=false",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
            "chr2\t6\t10\t-\tsecond\n",
            "chr1\t1\t4\t+\tfirst\n",
        )
    );
}

#[test]
fn bedtointervallist_can_drop_missing_contigs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let dictionary = tempdir.path().join("ref.dict");
    let bed = tempdir.path().join("targets.bed");
    let output = tempdir.path().join("targets.interval_list");
    fs::write(
        &dictionary,
        concat!("@HD\tVN:1.6\n", "@SQ\tSN:chr1\tLN:1000\n",),
    )
    .expect("dictionary fixture is written");
    fs::write(
        &bed,
        concat!(
            "chr_missing\t0\t3\tmissing\t0\t+\n",
            "chr1\t4\t8\tkept\t0\t+\n",
        ),
    )
    .expect("BED fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "BedToIntervalList",
        &format!("I={}", bed.display()),
        &format!("O={}", output.display()),
        &format!("SD={}", dictionary.display()),
        "DROP_MISSING_CONTIGS=true",
        "VALIDATION_STRINGENCY=SILENT",
    ])
    .assert()
    .success();

    assert_eq!(
        fs::read_to_string(output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t5\t8\t+\tkept\n",
        )
    );
}

#[test]
fn bedtointervallist_skips_or_keeps_zero_length_intervals_like_picard() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let dictionary = tempdir.path().join("ref.dict");
    let bed = tempdir.path().join("targets.bed");
    let skipped_output = tempdir.path().join("skipped.interval_list");
    let kept_output = tempdir.path().join("kept.interval_list");
    fs::write(
        &dictionary,
        concat!("@HD\tVN:1.6\n", "@SQ\tSN:chr1\tLN:1000\n",),
    )
    .expect("dictionary fixture is written");
    fs::write(
        &bed,
        concat!("chr1\t5\t5\tzero\t0\t+\n", "chr1\t5\t8\tkept\t0\t+\n",),
    )
    .expect("BED fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "BedToIntervalList",
            &format!("I={}", bed.display()),
            &format!("O={}", skipped_output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&skipped_output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t6\t8\t+\tkept\n",
        )
    );

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "BedToIntervalList",
            &format!("I={}", bed.display()),
            &format!("O={}", kept_output.display()),
            &format!("SD={}", dictionary.display()),
            "KEEP_LENGTH_ZERO_INTERVALS=true",
            "VALIDATION_STRINGENCY=SILENT",
        ])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&kept_output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t6\t5\t+\tzero\n",
            "chr1\t6\t8\t+\tkept\n",
        )
    );
}

#[test]
fn bedtointervallist_accepts_common_noop_sidecar_options() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let dictionary = tempdir.path().join("ref.dict");
    let bed = tempdir.path().join("targets.bed");
    let output = tempdir.path().join("targets.interval_list");
    fs::write(
        &dictionary,
        concat!("@HD\tVN:1.6\n", "@SQ\tSN:chr1\tLN:1000\n",),
    )
    .expect("dictionary fixture is written");
    fs::write(&bed, "chr1\t5\t8\tkept\t0\t+\n").expect("BED fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "BedToIntervalList",
            &format!("I={}", bed.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "COMPRESSION_LEVEL=1",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&output).expect("interval_list exists"),
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "chr1\t6\t8\t+\tkept\n",
        )
    );
    assert!(!tempdir.path().join("targets.interval_list.md5").exists());
    assert!(!tempdir.path().join("targets.interval_list.bai").exists());
    assert!(!tempdir.path().join("targets.interval_list.idx").exists());
}

#[test]
fn viewsam_converts_bam_to_sam() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let output_sam = tempdir.path().join("view.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output_sam.display()),
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output_sam).expect("ViewSam SAM output exists");
    assert!(output.contains("@SQ\tSN:chr1\tLN:1000"));
    assert_eq!(record_names(&output), vec!["read-a", "read-b"]);
}

#[test]
fn viewsam_records_only_omits_sam_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let output_sam = tempdir.path().join("records.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t512\tchr1\t20\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output_sam.display()),
            "RECORDS_ONLY=true",
            "PF_STATUS=PF",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output_sam).expect("ViewSam records-only output exists");
    assert!(!output.contains("@SQ\tSN:chr1\tLN:1000"));
    assert_eq!(record_names(&output), vec!["read-a"]);
}

#[test]
fn viewsam_header_only_omits_sam_records() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let output_sam = tempdir.path().join("header.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t512\tchr1\t20\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output_sam.display()),
            "HEADER_ONLY=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output_sam).expect("ViewSam header-only output exists");
    assert!(output.contains("@SQ\tSN:chr1\tLN:1000"));
    assert!(record_names(&output).is_empty());
}

#[test]
fn viewsam_filters_records_by_interval_list() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let intervals = tempdir.path().join("targets.interval_list");
    let output_sam = tempdir.path().join("view.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
            "read-c\t0\tchr2\t12\t60\t4M\t*\t0\t0\tGATC\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(
        &intervals,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
            "chr1\t12\t22\t+\ttarget\n",
        ),
    )
    .expect("interval list is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output_sam.display()),
            &format!("INTERVAL_LIST={}", intervals.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output_sam).expect("ViewSam interval output exists");
    assert_eq!(record_names(&output), vec!["read-a", "read-b"]);
}

#[test]
fn viewsam_filters_records_by_alignment_status() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input_sam = tempdir.path().join("input.sam");
    let input_bam = tempdir.path().join("input.bam");
    let output_sam = tempdir.path().join("aligned.sam");
    fs::write(
        &input_sam,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
            "read-b\t4\t*\t0\t0\t*\t*\t0\t0\tNNNN\t!!!!\n",
        ),
    )
    .expect("input SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortSam",
            &format!("I={}", input_sam.display()),
            &format!("O={}", input_bam.display()),
            "SO=coordinate",
        ])
        .assert()
        .success();

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ViewSam",
            &format!("I={}", input_bam.display()),
            &format!("O={}", output_sam.display()),
            "ALIGNMENT_STATUS=Aligned",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output_sam).expect("ViewSam aligned output exists");
    assert_eq!(record_names(&output), vec!["read-a"]);
}

#[test]
fn replacesamheader_streams_records_with_replacement_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let header = tempdir.path().join("header.sam");
    let output = tempdir.path().join("output.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(
        &header,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:2000\n",
            "@CO\treplacement header\n",
            "header-only\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
        ),
    )
    .expect("header SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ReplaceSamHeader",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("H={}", header.display()),
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("ReplaceSamHeader SAM output exists");
    assert!(output.contains("@HD\tVN:1.6\tSO:coordinate"));
    assert!(output.contains("@SQ\tSN:chr1\tLN:2000"));
    assert!(output.contains("@CO\treplacement header"));
    assert_eq!(record_names(&output), vec!["read-a"]);
}

#[test]
fn replacesamheader_writes_md5_sidecar_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let header = tempdir.path().join("header.sam");
    let output = tempdir.path().join("output.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(
        &header,
        concat!("@HD\tVN:1.6\tSO:coordinate\n", "@SQ\tSN:chr1\tLN:2000\n",),
    )
    .expect("header SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ReplaceSamHeader",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("HEADER={}", header.display()),
            "CREATE_MD5_FILE=true",
        ])
        .assert()
        .success();

    let md5 = fs::read_to_string(tempdir.path().join("output.sam.md5"))
        .expect("ReplaceSamHeader MD5 sidecar exists");
    assert_eq!(md5.trim().len(), 32);
}

#[test]
fn replacesamheader_accepts_common_runtime_sidecar_options_for_sam_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let header = tempdir.path().join("header.sam");
    let output = tempdir.path().join("output.sam");
    let reference = tempdir.path().join("ref.fa");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(
        &header,
        concat!("@HD\tVN:1.6\tSO:coordinate\n", "@SQ\tSN:chr1\tLN:2000\n",),
    )
    .expect("header SAM is written");
    fs::write(&reference, ">chr1\nACGTACGTACGT\n").expect("reference fixture is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ReplaceSamHeader",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("HEADER={}", header.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            &format!("REFERENCE_SEQUENCE={}", reference.display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("output.sam.md5").exists());
    assert!(!tempdir.path().join("output.sam.bai").exists());
    assert!(!tempdir.path().join("output.bai").exists());
}

#[test]
fn replacesamheader_rejects_mismatched_sort_order() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let header = tempdir.path().join("header.sam");
    let output = tempdir.path().join("output.sam");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        ),
    )
    .expect("input SAM is written");
    fs::write(
        &header,
        concat!("@HD\tVN:1.6\tSO:unsorted\n", "@SQ\tSN:chr1\tLN:2000\n",),
    )
    .expect("header SAM is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "ReplaceSamHeader",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("HEADER={}", header.display()),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sort orders of INPUT"));
}

#[test]
fn updatevcfsequencedictionary_replaces_contig_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("output.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=old,length=10>\n",
            "##source=test\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t7\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\tM5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tAS:GRCh38\n",
            "@SQ\tSN:chr2\tLN:2000\tUR:file:///ref.fa\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "UpdateVcfSequenceDictionary",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("updated VCF exists");
    assert!(output.contains(
        "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n"
    ));
    assert!(output.contains("##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n"));
    assert!(!output.contains("ID=old"));
    assert!(output.contains("chr2\t7\t.\tA\tC\t.\tPASS\t.\n"));
}

#[test]
fn updatevcfsequencedictionary_writes_index_for_vcf_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("output.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=old,length=10>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t7\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:2000\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "UpdateVcfSequenceDictionary",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("output.vcf.idx").exists());
    assert!(!tempdir.path().join("output.vcf.md5").exists());
}

#[test]
fn gathervcfs_concatenates_records_with_first_header() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let output = tempdir.path().join("gathered.vcf");
    let first_text = concat!(
        "##fileformat=VCFv4.2\n",
        "##contig=<ID=chr1,length=1000>\n",
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
        "chr1\t1\t.\tA\tC\t.\tPASS\t.\n",
    );
    fs::write(&first, first_text).expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t5\t.\tG\tT\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "GatherVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(output).expect("gathered VCF exists"),
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t1\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t5\t.\tG\tT\t.\tPASS\t.\n",
        )
    );
}

#[test]
fn gathervcfs_streams_gzip_input_and_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf.gz");
    let second = tempdir.path().join("second.vcf.gz");
    let output = tempdir.path().join("gathered.vcf.gz");
    for (path, record) in [
        (&first, "chr1\t1\t.\tA\tC\t.\tPASS\t.\n"),
        (&second, "chr1\t5\t.\tG\tT\t.\tPASS\t.\n"),
    ] {
        let file = fs::File::create(path).expect("gzip VCF can be created");
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder
            .write_all(
                format!(
                    "##fileformat=VCFv4.2\n\
                     ##contig=<ID=chr1,length=1000>\n\
                     #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n\
                     {record}"
                )
                .as_bytes(),
            )
            .expect("gzip VCF fixture is written");
        encoder.finish().expect("gzip VCF is finished");
    }

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "GatherVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let file = fs::File::open(&output).expect("gzip gathered VCF exists");
    let mut decoder = GzDecoder::new(file);
    let mut text = String::new();
    decoder
        .read_to_string(&mut text)
        .expect("gzip gathered VCF decodes");
    assert_eq!(
        text,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t1\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t5\t.\tG\tT\t.\tPASS\t.\n",
        )
    );
}

#[test]
fn gathervcfs_removes_temp_output_after_header_mismatch() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let output = tempdir.path().join("gathered.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tSAMPLE_A\n",
            "chr1\t1\t.\tA\tC\t.\tPASS\t.\t0/1\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tSAMPLE_B\n",
            "chr1\t5\t.\tG\tT\t.\tPASS\t.\t0/1\n",
        ),
    )
    .expect("second VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "GatherVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("different sample columns"));

    assert!(!output.exists());
    assert!(
        fs::read_dir(tempdir.path())
            .expect("tempdir can be read")
            .all(|entry| !entry
                .expect("dir entry exists")
                .file_name()
                .to_string_lossy()
                .starts_with(".turbo-picard-gathervcfs-"))
    );
}

#[test]
fn gathervcfs_writes_index_for_vcf_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let output = tempdir.path().join("gathered.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t1\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t5\t.\tG\tT\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "GatherVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("gathered.vcf.idx").exists());
    assert!(!tempdir.path().join("gathered.vcf.md5").exists());
}

#[test]
fn sortvcf_sorts_records_by_dictionary_order_and_position() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("sorted.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t9\t.\tA\tG\t.\tPASS\t.\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("sorted VCF exists");
    assert_eq!(
        output
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec![
            "chr1\t2\t.\tT\tC\t.\tPASS\t.",
            "chr1\t9\t.\tA\tG\t.\tPASS\t.",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.",
        ]
    );
}

#[test]
fn sortvcf_replaces_contig_header_from_explicit_dictionary() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("sorted.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n",
            "##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\tM5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tAS:GRCh38\n",
            "@SQ\tSN:chr2\tLN:2000\tUR:file:///ref.fa\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("sorted VCF exists");
    assert!(output.contains(
        "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n"
    ));
    assert!(output.contains("##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n"));
}

#[test]
fn sortvcf_writes_index_for_vcf_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("sorted.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("sorted.vcf.idx").exists());
    assert!(!tempdir.path().join("sorted.vcf.md5").exists());
}

#[test]
fn sortvcf_honors_tmp_dir_and_forced_external_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let sort_tmp = tempdir.path().join("sort-tmp");
    fs::create_dir(&sort_tmp).expect("sort temp dir is created");
    let input = tempdir.path().join("input.vcf");
    let output = tempdir.path().join("sorted.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t9\tfirst\tA\tG\t.\tPASS\t.\n",
            "chr1\t9\tsecond\tA\tT\t.\tPASS\t.\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("sorted VCF exists");
    assert_eq!(
        output
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec![
            "chr1\t2\t.\tT\tC\t.\tPASS\t.",
            "chr1\t9\tfirst\tA\tG\t.\tPASS\t.",
            "chr1\t9\tsecond\tA\tT\t.\tPASS\t.",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.",
        ]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("sort temp readable")
            .next()
            .is_none(),
        "external sort should clean temporary runs"
    );
}

#[test]
fn sortvcf_rejects_input_dictionary_that_differs_from_explicit_dictionary() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("sorted.vcf");
    fs::write(
        &input,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=10>\n",
            "##contig=<ID=chr2,length=20>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("input VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\tM5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tAS:GRCh38\n",
            "@SQ\tSN:chr2\tLN:2000\tUR:file:///ref.fa\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "SortVcf",
            &format!("I={}", input.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported SortVcf input sequence dictionary differs from expected dictionary",
        ));
}

#[test]
fn mergevcfs_merges_compatible_inputs_by_coordinate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let output = tempdir.path().join("merged.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("merged VCF exists");
    assert_eq!(
        output
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec![
            "chr1\t2\t.\tT\tC\t.\tPASS\t.",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.",
        ]
    );
}

#[test]
fn mergevcfs_streams_gzip_inputs_and_output() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf.gz");
    let second = tempdir.path().join("second.vcf.gz");
    let output = tempdir.path().join("merged.vcf.gz");
    for (path, record) in [
        (&first, "chr1\t5\tfirst\tA\tC\t.\tPASS\t.\n"),
        (&second, "chr2\t1\tsecond\tG\tT\t.\tPASS\t.\n"),
    ] {
        let file = fs::File::create(path).expect("gzip VCF can be created");
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder
            .write_all(
                format!(
                    "##fileformat=VCFv4.2\n\
                     ##contig=<ID=chr1,length=1000>\n\
                     ##contig=<ID=chr2,length=1000>\n\
                     #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n\
                     {record}"
                )
                .as_bytes(),
            )
            .expect("gzip VCF fixture is written");
        encoder.finish().expect("gzip VCF is finished");
    }

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert_eq!(
        read_gzip_to_string(output),
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t5\tfirst\tA\tC\t.\tPASS\t.\n",
            "chr2\t1\tsecond\tG\tT\t.\tPASS\t.\n",
        )
    );
}

#[test]
fn mergevcfs_honors_tmp_dir_and_forced_external_runs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let sort_tmp = tempdir.path().join("merge-tmp");
    fs::create_dir(&sort_tmp).expect("merge temp dir is created");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let output = tempdir.path().join("merged.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\tchr2-first\tA\tC\t.\tPASS\t.\n",
            "chr1\t9\tfirst\tA\tG\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t9\tsecond\tA\tT\t.\tPASS\t.\n",
            "chr1\t2\tlow\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            "MAX_RECORDS_IN_RAM=1",
            &format!("TMP_DIR={}", sort_tmp.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("merged VCF exists");
    assert_eq!(
        output
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec![
            "chr1\t2\tlow\tT\tC\t.\tPASS\t.",
            "chr1\t9\tfirst\tA\tG\t.\tPASS\t.",
            "chr1\t9\tsecond\tA\tT\t.\tPASS\t.",
            "chr2\t3\tchr2-first\tA\tC\t.\tPASS\t.",
        ]
    );
    assert!(
        fs::read_dir(&sort_tmp)
            .expect("merge temp readable")
            .next()
            .is_none(),
        "external merge sort should clean temporary runs"
    );
}

#[test]
fn mergevcfs_replaces_contig_header_from_explicit_dictionary() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("merged.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n",
            "##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n",
            "##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\tM5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tAS:GRCh38\n",
            "@SQ\tSN:chr2\tLN:2000\tUR:file:///ref.fa\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    let output = fs::read_to_string(output).expect("merged VCF exists");
    assert!(output.contains(
        "##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>\n"
    ));
    assert!(output.contains("##contig=<ID=chr2,length=2000,URI=file:///ref.fa>\n"));
}

#[test]
fn mergevcfs_writes_index_for_vcf_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("merged.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=1000>\n",
            "##contig=<ID=chr2,length=1000>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "@SQ\tSN:chr2\tLN:1000\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "CREATE_MD5_FILE=true",
            "CREATE_INDEX=true",
            "MAX_RECORDS_IN_RAM=1000",
            &format!("TMP_DIR={}", tempdir.path().display()),
            "USE_JDK_DEFLATER=true",
            "USE_JDK_INFLATER=true",
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .success();

    assert!(tempdir.path().join("merged.vcf.idx").exists());
    assert!(!tempdir.path().join("merged.vcf.md5").exists());
}

#[test]
fn mergevcfs_rejects_input_dictionary_that_differs_from_explicit_dictionary() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let first = tempdir.path().join("first.vcf");
    let second = tempdir.path().join("second.vcf");
    let dictionary = tempdir.path().join("reference.dict");
    let output = tempdir.path().join("merged.vcf");
    fs::write(
        &first,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=10>\n",
            "##contig=<ID=chr2,length=20>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr2\t3\t.\tA\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("first VCF is written");
    fs::write(
        &second,
        concat!(
            "##fileformat=VCFv4.2\n",
            "##contig=<ID=chr1,length=10>\n",
            "##contig=<ID=chr2,length=20>\n",
            "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
            "chr1\t2\t.\tT\tC\t.\tPASS\t.\n",
        ),
    )
    .expect("second VCF is written");
    fs::write(
        &dictionary,
        concat!(
            "@HD\tVN:1.6\n",
            "@SQ\tSN:chr1\tLN:1000\tM5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tAS:GRCh38\n",
            "@SQ\tSN:chr2\tLN:2000\tUR:file:///ref.fa\n",
        ),
    )
    .expect("dictionary is written");

    Command::cargo_bin("picard")
        .expect("binary exists")
        .args([
            "MergeVcfs",
            &format!("I={}", first.display()),
            &format!("I={}", second.display()),
            &format!("O={}", output.display()),
            &format!("SD={}", dictionary.display()),
            "VALIDATION_STRINGENCY=SILENT",
            "QUIET=true",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported MergeVcfs input sequence dictionary differs from expected dictionary",
        ));
}

#[test]
fn picard_binary_dispatches_markduplicates() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");
    fs::write(
        &input,
        concat!(
            "@HD\tVN:1.6\tSO:coordinate\n",
            "@SQ\tSN:chr1\tLN:1000\n",
            "read-a\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
            "read-b\t0\tchr1\t10\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tFFFFFFFFFF\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "MarkDuplicates",
        &format!("I={}", input.display()),
        &format!("O={}", output.display()),
        &format!("M={}", metrics.display()),
    ])
    .assert()
    .success();

    let output_sam = fs::read_to_string(&output).expect("output SAM exists");
    assert!(output_sam.contains("read-b\t1024\tchr1\t10"));
}

#[test]
fn picard_binary_supports_help_and_version_smoke_checks() {
    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("MarkDuplicates"));

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args(["MarkDuplicates", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("INPUT"));

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args(["IntervalListTools", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PADDING"))
        .stdout(predicate::str::contains("PADDING=0").not());

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

fn fallback_script(dir: &std::path::Path, exit_code: i32) -> std::path::PathBuf {
    let script = dir.join("fallback.sh");
    fs::write(
        &script,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\" > \"$TURBO_PICARD_FALLBACK_LOG\"\nexit {exit_code}\n"
        ),
    )
    .expect("fallback script is written");
    let mut permissions = fs::metadata(&script)
        .expect("fallback metadata exists")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).expect("fallback script is executable");
    script
}

fn read_gzip_to_string(path: std::path::PathBuf) -> String {
    let file = fs::File::open(path).expect("gzip output exists");
    let mut decoder = GzDecoder::new(file);
    let mut text = String::new();
    decoder
        .read_to_string(&mut text)
        .expect("gzip output is readable");
    text
}

fn record_names(sam: &str) -> Vec<&str> {
    sam.lines()
        .filter(|line| !line.starts_with('@'))
        .map(|line| line.split('\t').next().expect("record has qname"))
        .collect()
}
