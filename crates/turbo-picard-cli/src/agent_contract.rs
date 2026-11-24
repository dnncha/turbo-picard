use turbo_picard_core::picard_args::parse_picard_args_for_command;

pub(crate) fn declared_outputs(command: &str, args: &[String]) -> Result<Vec<String>, String> {
    let parsed = parse_picard_args_for_command(command, args).map_err(|error| error.to_string())?;
    Ok(parsed
        .into_iter()
        .filter(|arg| !arg.value.is_empty() && is_output_argument(command, &arg.canonical_key))
        .map(|arg| format!("{}={}", arg.raw_key, arg.value))
        .collect())
}

pub(crate) fn declared_inputs(command: &str, args: &[String]) -> Result<Vec<String>, String> {
    let parsed = parse_picard_args_for_command(command, args).map_err(|error| error.to_string())?;
    Ok(parsed
        .into_iter()
        .filter(|arg| !arg.value.is_empty() && is_input_argument(command, &arg.canonical_key))
        .map(|arg| arg.value)
        .collect())
}

fn is_input_argument(command: &str, key: &str) -> bool {
    key == "INPUT"
        || matches!(
            (command, key),
            ("FastqToSam", "FASTQ" | "FASTQ2")
                | ("IntervalListTools", "SECOND_INPUT")
                | ("ReplaceSamHeader", "HEADER")
        )
}

fn is_output_argument(command: &str, key: &str) -> bool {
    key == "OUTPUT"
        || matches!(
            (command, key),
            ("MarkDuplicates", "METRICS_FILE")
                | (
                    "SamToFastq",
                    "FASTQ" | "SECOND_END_FASTQ" | "UNPAIRED_FASTQ" | "OUTPUT_DIR"
                )
                | ("CollectInsertSizeMetrics", "HISTOGRAM_FILE")
                | ("CollectGcBiasMetrics", "SUMMARY_OUTPUT")
                | (
                    "CollectHsMetrics",
                    "PER_TARGET_COVERAGE" | "PER_BASE_COVERAGE"
                )
                | ("LiftoverVcf", "REJECT")
                | (
                    "CollectBaseDistributionByCycle"
                        | "CollectGcBiasMetrics"
                        | "CollectRnaSeqMetrics"
                        | "MeanQualityByCycle"
                        | "QualityScoreDistribution",
                    "CHART_OUTPUT"
                )
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inspect(command: &str, args: &[&str]) -> Vec<String> {
        declared_outputs(
            command,
            &args.iter().map(ToString::to_string).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn input_fastqs_are_never_reported_as_outputs() {
        assert_eq!(
            inspect("FastqToSam", &["FASTQ=a.fq", "F2=b.fq", "O=out.bam"]),
            ["O=out.bam"]
        );
        assert_eq!(
            inspect("SamToFastq", &["F=a.fq", "F2=b.fq", "FU=u.fq"]),
            ["F=a.fq", "F2=b.fq", "FU=u.fq"]
        );
    }

    #[test]
    fn numeric_aliases_are_not_metrics_files() {
        assert_eq!(
            inspect(
                "CollectInsertSizeMetrics",
                &["M=0.05", "O=metrics", "H=plot.pdf"]
            ),
            ["O=metrics", "H=plot.pdf"]
        );
        assert_eq!(
            inspect(
                "IntervalListTools",
                &["M=INTERVAL_COUNT", "O=out.interval_list"]
            ),
            ["O=out.interval_list"]
        );
        assert_eq!(
            inspect("MarkDuplicates", &["M=metrics", "O=out.bam"]),
            ["M=metrics", "O=out.bam"]
        );
    }

    #[test]
    fn all_supported_argument_syntaxes_and_repeated_outputs_are_preserved() {
        assert_eq!(
            inspect(
                "MarkDuplicates",
                &[
                    "--INPUT",
                    "input.bam",
                    "--OUTPUT=one.bam",
                    "-O",
                    "two.bam",
                    "--METRICS_FILE",
                    "metrics with spaces.txt"
                ]
            ),
            [
                "OUTPUT=one.bam",
                "O=two.bam",
                "METRICS_FILE=metrics with spaces.txt"
            ]
        );
        assert_eq!(
            inspect(
                "CollectGcBiasMetrics",
                &["CHART=chart.pdf", "S=summary.txt"]
            ),
            ["CHART=chart.pdf", "S=summary.txt"]
        );
        assert_eq!(
            inspect("LiftoverVcf", &["REJECT=reject.vcf"]),
            ["REJECT=reject.vcf"]
        );
        assert_eq!(
            inspect("ReplaceSamHeader", &["H=header.sam"]),
            [] as [String; 0]
        );
    }

    #[test]
    fn malformed_argument_syntax_fails_without_inspecting_files() {
        for args in [
            vec!["--OUTPUT"],
            vec!["O"],
            vec!["--OUTPUT", "--INPUT"],
            vec!["=file"],
        ] {
            let args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
            assert!(declared_outputs("MarkDuplicates", &args).is_err());
        }
        assert_eq!(
            inspect("MarkDuplicates", &["I=does-not-exist.bam", "O=planned.bam"]),
            ["O=planned.bam"]
        );
    }

    #[test]
    fn gatk_kebab_case_outputs_and_case_insensitive_inputs_are_recognized() {
        assert_eq!(
            inspect(
                "MarkDuplicates",
                &[
                    "-i",
                    "in.bam",
                    "--output",
                    "marked.bam",
                    "--metrics-file=metrics.txt",
                    "--create-index",
                    "true",
                ]
            ),
            ["output=marked.bam", "metrics-file=metrics.txt"]
        );
        assert_eq!(
            inspect(
                "CollectHsMetrics",
                &[
                    "--input",
                    "in.bam",
                    "--output",
                    "hs.txt",
                    "--per-target-coverage",
                    "targets.tsv",
                ]
            ),
            ["output=hs.txt", "per-target-coverage=targets.tsv"]
        );
        let inputs = declared_inputs(
            "MarkDuplicates",
            &[
                "i=a.bam".to_string(),
                "--input".to_string(),
                "b.bam".to_string(),
                "--I=c.bam".to_string(),
                "-i".to_string(),
                "d.bam".to_string(),
                "O=out.bam".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(inputs, ["a.bam", "b.bam", "c.bam", "d.bam"]);
        let fastq_inputs = declared_inputs(
            "FastqToSam",
            &[
                "--fastq".to_string(),
                "r1.fq".to_string(),
                "-F2".to_string(),
                "r2.fq".to_string(),
                "O=u.bam".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(fastq_inputs, ["r1.fq", "r2.fq"]);
    }
}
