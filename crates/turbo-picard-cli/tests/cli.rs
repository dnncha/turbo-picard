use assert_cmd::Command;
use flate2::read::GzDecoder;
use predicates::prelude::*;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;

#[test]
fn unsupported_command_fails_clearly() {
    let mut cmd = Command::cargo_bin("turbo-picard").expect("binary exists");
    cmd.arg("ValidateSamFile")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported Picard command: ValidateSamFile",
        ));
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
    .args(["ValidateSamFile", "I=in.bam", "MODE=SUMMARY"])
    .assert()
    .code(17);

    let fallback_args = fs::read_to_string(log).expect("fallback log exists");
    assert_eq!(fallback_args, "ValidateSamFile\nI=in.bam\nMODE=SUMMARY\n");
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
    ])
    .assert()
    .success();

    assert!(output.exists());
    assert!(tempdir.path().join("coordinate.bam.md5").exists());
    assert!(tempdir.path().join("coordinate.bai").exists());
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
            "pair-a\t77\t*\t0\t0\t*\t*\t0\t0\tAAAA\tFFFF\n",
            "pair-a\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tHHHH\n",
        ),
    )
    .expect("input fixture is written");

    let mut cmd = Command::cargo_bin("picard").expect("binary exists");
    cmd.args([
        "SamToFastq",
        &format!("I={}", input.display()),
        &format!("FASTQ={}", first_fastq.display()),
        &format!("SECOND_END_FASTQ={}", second_fastq.display()),
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

fn record_names(sam: &str) -> Vec<&str> {
    sam.lines()
        .filter(|line| !line.starts_with('@'))
        .map(|line| line.split('\t').next().expect("record has qname"))
        .collect()
}
