//! Read-only command inspection. Output roles are command-specific: for example,
//! F2 is an input to FastqToSam but an output of SamToFastq, and M is not always
//! a metrics filename. Reuse the execution parser rather than inventing syntax.
use turbo_picard_core::picard_args::normalize_picard_args_for_command;

pub(crate) fn declared_outputs(command: &str, args: &[String]) -> Result<Vec<String>, String> {
    // This checks argument syntax only, not option support or input contents.
    normalize_picard_args_for_command(command, args).map_err(|error| error.to_string())?;
    let mut outputs = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let (key, value, width) = if let Some(long) = arg.strip_prefix("--") {
            if let Some((key, value)) = long.split_once('=') {
                (key, value, 1)
            } else {
                (long, args[index + 1].as_str(), 2)
            }
        } else if let Some(short) = arg.strip_prefix('-') {
            (short, args[index + 1].as_str(), 2)
        } else {
            let (key, value) = arg.split_once('=').expect("syntax checked above");
            (key, value, 1)
        };
        let normalized = normalize_picard_args_for_command(command, &args[index..index + width])
            .map_err(|error| error.to_string())?;
        let canonical = normalized.keys().next().expect("one checked argument");
        if !value.is_empty() && is_output_argument(command, canonical) {
            // Preserve spelling and order in the existing schema-v1 field.
            outputs.push(format!("{key}={value}"));
        }
        index += width;
    }
    Ok(outputs)
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
            &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
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
        assert!(inspect("ReplaceSamHeader", &["H=header.sam"]).is_empty());
    }

    #[test]
    fn malformed_argument_syntax_fails_without_inspecting_files() {
        for args in [
            vec!["--OUTPUT"],
            vec!["O"],
            vec!["--OUTPUT", "--INPUT"],
            vec!["=file"],
        ] {
            let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
            assert!(declared_outputs("MarkDuplicates", &args).is_err());
        }
        assert_eq!(
            inspect("MarkDuplicates", &["I=does-not-exist.bam", "O=planned.bam"]),
            ["O=planned.bam"]
        );
    }
}
