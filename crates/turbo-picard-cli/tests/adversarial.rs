use rust_htslib::bam::{self, Read};
use std::fs;

use support::{arg, cli, convert, flags};

const HEADER: &str = "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n@RG\tID:rg\tSM:sample\tLB:lib\tPL:ILLUMINA\n";
const FRAGMENT: &str = "frag\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\n";

#[test]
fn duplicate_selection_is_format_independent_and_keeps_the_best_read() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.sam");
    fs::write(
        &input,
        format!(
            "{HEADER}{}{}",
            FRAGMENT.replace("frag", "low").replace("IIII", "!!!!"),
            FRAGMENT.replace("frag", "high")
        ),
    )
    .unwrap();
    let bam = dir.path().join("input.bam");
    convert(&input, &bam);
    for input in [&input, &bam] {
        for remove in [false, true] {
            let output = dir.path().join("output.bam");
            cli(&[
                "MarkDuplicates".into(),
                arg("I", input),
                arg("O", &output),
                arg("M", &dir.path().join("metrics")),
                "READ_NAME_REGEX=null".into(),
                format!("REMOVE_DUPLICATES={remove}"),
            ])
            .assert()
            .success();
            let observed = flags(&output);
            assert_eq!(observed.last(), Some(&(b"high".to_vec(), 0)));
            assert_eq!(observed.len(), if remove { 1 } else { 2 });
            if !remove {
                assert_eq!(observed[0].1, 1024);
            }
        }
    }
}

#[test]
fn fragment_duplicates_are_identical_across_fast_compact_and_external_plans() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.sam");
    let records = format!(
        "pair\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\tRG:Z:rg\n{FRAGMENT}pair\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tACGT\tIIII\tRG:Z:rg\n"
    );
    for external in [false, true] {
        let extra = if external {
            "@RG\tID:other\tSM:sample\tLB:lib\tPL:ILLUMINA\n"
        } else {
            ""
        };
        fs::write(&input, format!("{HEADER}{extra}{records}")).unwrap();
        let bam = dir.path().join("input.bam");
        convert(&input, &bam);
        for tag in [false, true] {
            let output = dir.path().join("output.bam");
            cli(&[
                "MarkDuplicates".into(),
                arg("I", &bam),
                arg("O", &output),
                arg("M", &dir.path().join("metrics")),
                "READ_NAME_REGEX=null".into(),
                format!("TAG_DUPLICATE_SET_MEMBERS={tag}"),
            ])
            .assert()
            .success();
            assert_eq!(
                flags(&output).iter().map(|(_, f)| *f).collect::<Vec<_>>(),
                [99, 1024, 147]
            );
        }
    }
}

#[test]
fn duplicate_marking_rejects_incompatible_dictionaries_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.sam");
    let b = dir.path().join("b.sam");
    fs::write(&a, format!("{HEADER}{FRAGMENT}")).unwrap();
    fs::write(&b, format!("{HEADER}{FRAGMENT}").replace("chr1", "chr2")).unwrap();
    let a_bam = dir.path().join("a.bam");
    let b_bam = dir.path().join("b.bam");
    convert(&a, &a_bam);
    convert(&b, &b_bam);
    let output = dir.path().join("output.bam");
    fs::write(&output, "existing output").unwrap();
    cli(&[
        "MarkDuplicates".into(),
        arg("I", &a_bam),
        arg("I", &b_bam),
        arg("O", &output),
        arg("M", &dir.path().join("metrics")),
    ])
    .assert()
    .failure();
    assert_eq!(fs::read_to_string(output).unwrap(), "existing output");
}

#[test]
fn duplicate_marking_rejects_reference_checksum_conflicts_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = Vec::new();
    for (name, checksum) in [
        ("a", "00000000000000000000000000000001"),
        ("b", "00000000000000000000000000000002"),
    ] {
        let sam = dir.path().join(format!("{name}.sam"));
        fs::write(
            &sam,
            format!(
                "{}{}",
                HEADER.replace("LN:1000", &format!("LN:1000\tM5:{checksum}")),
                FRAGMENT.replace("frag", name)
            ),
        )
        .unwrap();
        let bam = sam.with_extension("bam");
        convert(&sam, &bam);
        inputs.push((sam, bam));
    }
    for binary in [false, true] {
        let output = dir.path().join("existing.bam");
        let metrics = dir.path().join("existing.metrics");
        fs::write(&output, "existing alignment").unwrap();
        fs::write(&metrics, "existing metrics").unwrap();
        let input = |index: usize| {
            if binary {
                &inputs[index].1
            } else {
                &inputs[index].0
            }
        };
        cli(&[
            "MarkDuplicates".into(),
            arg("I", input(0)),
            arg("I", input(1)),
            arg("O", &output),
            arg("M", &metrics),
        ])
        .assert()
        .failure();
        assert_eq!(fs::read_to_string(output).unwrap(), "existing alignment");
        assert_eq!(fs::read_to_string(metrics).unwrap(), "existing metrics");
    }
}

#[test]
fn read_group_collisions_preserve_library_identity_in_records_and_headers() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = Vec::new();
    for (name, library, position) in [("a", "libA", 10), ("b", "libB", 20)] {
        let input = dir.path().join(format!("{name}.sam"));
        fs::write(
            &input,
            format!(
                "{}{}",
                HEADER.replace("LB:lib", &format!("LB:{library}")),
                FRAGMENT
                    .replace("frag", name)
                    .replace("\t10\t", &format!("\t{position}\t"))
            ),
        )
        .unwrap();
        let bam = dir.path().join(format!("{name}.bam"));
        convert(&input, &bam);
        inputs.push(bam);
    }
    let output = dir.path().join("out.sam");
    cli(&[
        "MarkDuplicates".into(),
        arg("I", &inputs[0]),
        arg("I", &inputs[1]),
        arg("O", &output),
        arg("M", &dir.path().join("metrics")),
        "READ_NAME_REGEX=null".into(),
    ])
    .assert()
    .success();
    let mut reader = bam::Reader::from_path(output).unwrap();
    let header = String::from_utf8(reader.header().as_bytes().to_vec()).unwrap();
    let libraries: std::collections::BTreeMap<_, _> = header
        .lines()
        .filter(|l| l.starts_with("@RG\t"))
        .map(|l| {
            let fields: std::collections::BTreeMap<_, _> = l
                .split('\t')
                .skip(1)
                .filter_map(|f| f.split_once(':'))
                .collect();
            (fields["ID"].to_string(), fields["LB"].to_string())
        })
        .collect();
    for record in reader.records() {
        let record = record.unwrap();
        let bam::record::Aux::String(group) = record.aux(b"RG").unwrap() else {
            panic!("RG string")
        };
        assert_eq!(
            libraries[group],
            if record.qname() == b"a" {
                "libA"
            } else {
                "libB"
            }
        );
    }
}

#[test]
fn missing_qualities_do_not_change_sequence_yield_or_create_quality_yield() {
    let dir = tempfile::tempdir().unwrap();
    let sam = dir.path().join("input.sam");
    fs::write(
        &sam,
        format!("{HEADER}read\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\tRG:Z:rg\n"),
    )
    .unwrap();
    let bam = dir.path().join("input.bam");
    convert(&sam, &bam);
    for input in [&sam, &bam] {
        let output = dir.path().join("metrics");
        cli(&[
            "CollectQualityYieldMetrics".into(),
            arg("I", input),
            arg("O", &output),
        ])
        .assert()
        .success();
        let text = fs::read_to_string(&output).unwrap();
        let rows = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(rows[1], "1\t1\t4\t4\t4\t0\t0\t0\t0\t0\t0");
    }
}

#[test]
fn sortsam_verifies_records_instead_of_trusting_the_declared_order() {
    let dir = tempfile::tempdir().unwrap();
    let sam = dir.path().join("input.sam");
    fs::write(
        &sam,
        format!(
            "{HEADER}{}{}",
            FRAGMENT.replace("\t10\t", "\t20\t"),
            FRAGMENT
        ),
    )
    .unwrap();
    let bam = dir.path().join("input.bam");
    convert(&sam, &bam);
    for extension in ["sam", "bam"] {
        let output = dir.path().join(format!("out.{extension}"));
        cli(&[
            "SortSam".into(),
            arg("I", &bam),
            arg("O", &output),
            "SO=coordinate".into(),
        ])
        .assert()
        .success();
        let positions = bam::Reader::from_path(output)
            .unwrap()
            .records()
            .map(|r| r.unwrap().pos())
            .collect::<Vec<_>>();
        assert_eq!(positions, [9, 19]);
    }
}

#[test]
fn sam_summary_validation_checks_nm_against_the_supplied_reference() {
    let dir = tempfile::tempdir().unwrap();
    let reference = dir.path().join("ref.fa");
    fs::write(&reference, format!(">chr1\n{}\n", "ACGT".repeat(250))).unwrap();
    let sam = dir.path().join("input.sam");
    fs::write(
        &sam,
        format!("{HEADER}read\t0\tchr1\t1\t60\t4M\t*\t0\t0\tTTTT\tIIII\tRG:Z:rg\tNM:i:0\n"),
    )
    .unwrap();
    let bam = dir.path().join("input.bam");
    convert(&sam, &bam);
    for input in [&sam, &bam] {
        let output = dir.path().join("validation");
        cli(&[
            "ValidateSamFile".into(),
            arg("I", input),
            arg("R", &reference),
            arg("O", &output),
            "MODE=SUMMARY".into(),
        ])
        .assert()
        .code(3);
        assert!(
            fs::read_to_string(output)
                .unwrap()
                .contains("ERROR:INVALID_TAG_NM\t1")
        );
    }
}

#[test]
fn mixed_alignment_metrics_include_both_histograms_and_noise_reads() {
    let dir = tempfile::tempdir().unwrap();
    let sam = dir.path().join("input.sam");
    fs::write(&sam, format!("{HEADER}{FRAGMENT}pair\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\tRG:Z:rg\npair\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tACGT\tIIII\tRG:Z:rg\nnoise\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\tXN:i:1\n")).unwrap();
    let bam = dir.path().join("input.bam");
    convert(&sam, &bam);
    for input in [&sam, &bam] {
        for combined in [false, true] {
            let output = dir.path().join("metrics");
            let mut args = vec![
                if combined {
                    "CollectMultipleMetrics".into()
                } else {
                    "CollectAlignmentSummaryMetrics".into()
                },
                arg("I", input),
                arg("O", &output),
            ];
            if combined {
                args.push("PROGRAM=null".into());
                args.push("PROGRAM=CollectAlignmentSummaryMetrics".into());
            }
            cli(&args).assert().success();
            let path = if combined {
                output.with_extension("alignment_summary_metrics")
            } else {
                output
            };
            let text = fs::read_to_string(path).unwrap();
            let row = text
                .lines()
                .find(|l| l.starts_with("UNPAIRED\t"))
                .unwrap()
                .split('\t')
                .collect::<Vec<_>>();
            assert_eq!(row[1], "2");
            assert_eq!(row[4], "1");
            assert!(text.contains("READ_LENGTH\tPAIRED_TOTAL_LENGTH_COUNT\tPAIRED_ALIGNED_LENGTH_COUNT\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n"));
            assert!(text.contains("4\t2\t2\t2\t1\n"));
        }
    }
}

#[test]
fn vcf_mergers_preserve_later_info_format_and_filter_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let header = "##fileformat=VCFv4.2\n##contig=<ID=chr1,length=1000>\n";
    let columns = "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample\n";
    let a = dir.path().join("a.vcf");
    let b = dir.path().join("b.vcf");
    let definitions = "##INFO=<ID=X,Number=1,Type=Integer,Description=\"second, shard\">\n##FORMAT=<ID=GT,Number=1,Type=String,Description=\"genotype\">\n##FILTER=<ID=Low,Description=\"low\">\n";
    fs::write(
        &a,
        format!("{header}{columns}chr1\t10\t.\tA\tC\t50\tPASS\t.\t.\t.\n"),
    )
    .unwrap();
    fs::write(
        &b,
        format!("{header}{definitions}{columns}chr1\t20\t.\tA\tT\t50\tLow\tX=7\tGT\t0/1\n"),
    )
    .unwrap();
    for command in ["SortVcf", "MergeVcfs"] {
        for spill in [1, 500_000] {
            let output = dir.path().join("out.vcf");
            cli(&[
                command.into(),
                arg("I", &a),
                arg("I", &b),
                arg("O", &output),
                format!("MAX_RECORDS_IN_RAM={spill}"),
            ])
            .assert()
            .success();
            let text = fs::read_to_string(&output).unwrap();
            for definition in definitions.lines() {
                assert!(text.contains(definition));
            }
            assert!(text.contains("\tX=7\tGT\t0/1\n"));
        }
    }
}

#[test]
fn gather_rejects_unordered_overlapping_and_undefined_shards_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let header = "##fileformat=VCFv4.2\n##contig=<ID=chr1,length=1000>\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n";
    let a = dir.path().join("a.vcf");
    let b = dir.path().join("b.vcf");
    fs::write(&a, format!("{header}chr1\t20\t.\tA\tC\t50\tPASS\t.\n")).unwrap();
    for position in [10, 20] {
        fs::write(
            &b,
            format!("{header}chr1\t{position}\t.\tA\tT\t50\tPASS\t.\n"),
        )
        .unwrap();
        let output = dir.path().join("out.vcf");
        fs::write(&output, "existing output").unwrap();
        cli(&[
            "GatherVcfs".into(),
            arg("I", &a),
            arg("I", &b),
            arg("O", &output),
            "CREATE_INDEX=false".into(),
        ])
        .assert()
        .failure();
        assert_eq!(fs::read_to_string(output).unwrap(), "existing output");
    }
}

#[test]
fn structural_validation_does_not_certify_malformed_or_unordered_sam() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.sam");
    let output = dir.path().join("validation");
    let valid = FRAGMENT.trim_end().to_string() + "\tNM:i:0\n";
    for (records, issue) in [
        (valid.replace("4M", "5M"), "MISMATCH_CIGAR_SEQ_LENGTH"),
        (valid.replace("IIII", "II"), "MISMATCH_SEQ_QUAL_LENGTH"),
        (
            valid.replace("\t10\t", "\t1000\t"),
            "CIGAR_MAPS_OFF_REFERENCE",
        ),
        (
            valid.replace("\t10\t", "\t20\t") + &valid,
            "RECORD_OUT_OF_ORDER",
        ),
    ] {
        fs::write(&input, format!("{HEADER}{records}")).unwrap();
        cli(&[
            "ValidateSamFile".into(),
            arg("I", &input),
            arg("O", &output),
            "MODE=SUMMARY".into(),
        ])
        .assert()
        .code(3);
        assert!(fs::read_to_string(&output).unwrap().contains(issue));
        cli(&[
            "ValidateSamFile".into(),
            arg("I", &input),
            arg("O", &output),
            "MODE=SUMMARY".into(),
            format!("IGNORE={issue}"),
        ])
        .assert()
        .success();
    }
}

#[test]
fn validation_exit_status_distinguishes_warnings_errors_and_mixed_reports() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.sam");
    for (text, expected) in [
        (format!("{HEADER}{FRAGMENT}"), 1),
        (
            format!("{HEADER}{}", FRAGMENT.replace("\t10\t", "\t1000\t")),
            2,
        ),
        (
            format!(
                "{HEADER}{}",
                FRAGMENT
                    .replace("\t10\t", "\t1000\t")
                    .trim_end()
                    .to_string()
                    + "\tNM:i:0\n"
            ),
            3,
        ),
    ] {
        fs::write(&input, text).unwrap();
        cli(&[
            "ValidateSamFile".into(),
            arg("I", &input),
            "MODE=SUMMARY".into(),
        ])
        .assert()
        .code(expected);
    }
}

#[test]
fn missing_qualities_never_create_quality_histogram_bins_or_cycle_means() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("noqual.sam");
    fs::write(
        &input,
        format!("{HEADER}read\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\tRG:Z:rg\n"),
    )
    .unwrap();
    let bam = dir.path().join("noqual.bam");
    convert(&input, &bam);
    for input in [&input, &bam] {
        for command in ["MeanQualityByCycle", "QualityScoreDistribution"] {
            let output = dir.path().join("metrics");
            cli(&[
                command.into(),
                arg("I", input),
                arg("O", &output),
                arg("CHART", &dir.path().join("chart.pdf")),
            ])
            .assert()
            .success();
            let text = fs::read_to_string(&output).unwrap();
            assert!(
                text.lines()
                    .all(|line| line.trim().is_empty() || line.starts_with('#'))
            );
        }
    }
}

#[test]
fn mate_repair_handles_missing_primaries_supplementaries_and_secondary_records_natively() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mates.sam");
    let records = concat!(
        "complete\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\n",
        "complete\t355\tchr1\t20\t20\t4M\t*\t0\t0\tGGGG\tIIII\n",
        "complete\t147\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tIIII\n",
        "orphan\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\n",
        "orphan\t2147\tchr1\t100\t60\t4M\t=\t30\t24\tGGGG\tIIII\n",
        "supponly\t2147\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\n",
        "supponly\t2195\tchr1\t30\t60\t4M\t=\t10\t-24\tGGGG\tIIII\n",
    );
    fs::write(
        &input,
        format!(
            "{}{records}",
            HEADER.replace("SO:coordinate", "SO:queryname")
        ),
    )
    .unwrap();
    let output = dir.path().join("fixed.sam");
    cli(&[
        "FixMateInformation".into(),
        arg("I", &input),
        arg("O", &output),
        "SO=queryname".into(),
    ])
    .assert()
    .success();
    let text = fs::read_to_string(&output).unwrap();
    assert!(
        text.contains("complete\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\tMC:Z:4M\tMQ:i:60")
    );
    assert!(text.contains("complete\t355\tchr1\t20\t20\t4M\t*\t0\t0\tGGGG\tIIII\n"));
    for line in records
        .lines()
        .filter(|line| !line.starts_with("complete\t"))
    {
        assert!(text.contains(line));
    }
    cli(&[
        "FixMateInformation".into(),
        arg("I", &input),
        arg("O", &output),
        "SO=queryname".into(),
        "IGNORE_MISSING_MATES=false".into(),
    ])
    .assert()
    .failure()
    .stderr(predicates::str::contains(
        "Missing second read of pair: orphan",
    ));
}

#[test]
fn mate_insert_size_uses_five_prime_ends_for_tandem_pairs() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mates.sam");
    fs::write(&input, format!("{}pair\t67\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\npair\t131\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tIIII\n", HEADER.replace("SO:coordinate", "SO:queryname"))).unwrap();
    let output = dir.path().join("fixed.sam");
    cli(&[
        "FixMateInformation".into(),
        arg("I", &input),
        arg("O", &output),
        "SO=queryname".into(),
    ])
    .assert()
    .success();
    let sizes = bam::Reader::from_path(output)
        .unwrap()
        .records()
        .map(|record| record.unwrap().insert_size())
        .collect::<Vec<_>>();
    assert_eq!(sizes, [21, -21]);
}

#[test]
fn native_only_errors_preserve_the_actual_rejection_reason() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.sam");
    fs::write(&input, format!("{HEADER}{FRAGMENT}")).unwrap();
    cli(&[
        "FixMateInformation".into(),
        arg("I", &input),
        arg("O", &dir.path().join("out.sam")),
        "SORT_ORDER=unknown".into(),
    ])
    .assert()
    .failure()
    .stderr(predicates::str::contains(
        "unsupported FixMateInformation SORT_ORDER=unknown",
    ))
    .stderr(predicates::str::contains(
        "upstream Picard fallback is disabled",
    ));
}

#[cfg(test)]
mod support {
    use assert_cmd::Command;
    use rust_htslib::bam::{self, Read};
    use std::path::Path;

    pub fn cli(args: &[String]) -> Command {
        let mut command = Command::cargo_bin("turbo-picard").unwrap();
        command.env("TURBO_PICARD_REQUIRE_NATIVE", "1").args(args);
        command
    }

    pub fn arg(key: &str, path: &Path) -> String {
        format!("{key}={}", path.display())
    }

    pub fn convert(input: &Path, output: &Path) {
        cli(&["ViewSam".into(), arg("I", input), arg("O", output)])
            .assert()
            .success();
    }

    pub fn flags(path: &Path) -> Vec<(Vec<u8>, u16)> {
        bam::Reader::from_path(path)
            .unwrap()
            .records()
            .map(|r| {
                let r = r.unwrap();
                (r.qname().to_vec(), r.flags())
            })
            .collect()
    }
}
