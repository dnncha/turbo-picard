#![forbid(unsafe_code)]

use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;

const DUPLICATE_FLAG: u16 = 0x400;
const UNMAPPED_FLAG: u16 = 0x4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesSummary {
    pub records_examined: u64,
    pub duplicate_records: u64,
    pub unmapped_records: u64,
}

#[derive(Debug)]
pub enum MarkDuplicatesError {
    UnsupportedInputFormat(String),
    Io(std::io::Error),
    MalformedSam { line_number: usize, reason: String },
}

impl fmt::Display for MarkDuplicatesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedInputFormat(path) => write!(
                f,
                "unsupported MarkDuplicates input format for {path}; this engine milestone supports SAM text only"
            ),
            Self::Io(error) => write!(f, "{error}"),
            Self::MalformedSam {
                line_number,
                reason,
            } => write!(f, "malformed SAM at line {line_number}: {reason}"),
        }
    }
}

impl std::error::Error for MarkDuplicatesError {}

impl From<std::io::Error> for MarkDuplicatesError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DuplicateKey {
    reference_name: String,
    position: String,
    cigar: String,
    mate_reference_name: String,
    mate_position: String,
    template_length: String,
    reverse_strand: bool,
}

pub fn run(config: &MarkDuplicatesConfig) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    ensure_sam_input(&config.input)?;

    let input = fs::read_to_string(&config.input)?;
    let mut seen = HashMap::<DuplicateKey, usize>::new();
    let mut output = String::with_capacity(input.len());
    let mut summary = MarkDuplicatesSummary {
        records_examined: 0,
        duplicate_records: 0,
        unmapped_records: 0,
    };

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if line.starts_with('@') {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let mut fields = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if fields.len() < 11 {
            return Err(MarkDuplicatesError::MalformedSam {
                line_number,
                reason: "expected at least 11 tab-delimited fields".to_string(),
            });
        }

        let mut flag = fields[1]
            .parse::<u16>()
            .map_err(|_| MarkDuplicatesError::MalformedSam {
                line_number,
                reason: format!("invalid FLAG value: {}", fields[1]),
            })?;

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            output.push_str(line);
            output.push('\n');
            continue;
        }

        summary.records_examined += 1;
        let key = duplicate_key(&fields, flag);
        let seen_count = seen.entry(key).or_insert(0);
        let duplicate = *seen_count > 0;
        *seen_count += 1;

        if duplicate {
            summary.duplicate_records += 1;
            flag |= DUPLICATE_FLAG;
            fields[1] = flag.to_string();
        }

        if !(duplicate && config.remove_duplicates) {
            output.push_str(&fields.join("\t"));
            output.push('\n');
        }
    }

    fs::write(&config.output, output)?;
    fs::write(&config.metrics_file, metrics_text(&summary))?;
    Ok(summary)
}

fn ensure_sam_input(input: &str) -> Result<(), MarkDuplicatesError> {
    if Path::new(input)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sam"))
    {
        Ok(())
    } else {
        Err(MarkDuplicatesError::UnsupportedInputFormat(
            input.to_string(),
        ))
    }
}

fn duplicate_key(fields: &[String], flag: u16) -> DuplicateKey {
    DuplicateKey {
        reference_name: fields[2].clone(),
        position: fields[3].clone(),
        cigar: fields[5].clone(),
        mate_reference_name: fields[6].clone(),
        mate_position: fields[7].clone(),
        template_length: fields[8].clone(),
        reverse_strand: flag & 0x10 != 0,
    }
}

fn metrics_text(summary: &MarkDuplicatesSummary) -> String {
    let percent_duplication = if summary.records_examined == 0 {
        0.0
    } else {
        summary.duplicate_records as f64 / summary.records_examined as f64
    };

    format!(
        concat!(
            "## METRICS CLASS\tpicard.sam.DuplicationMetrics\n",
            "LIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\tSECONDARY_OR_SUPPLEMENTARY_RDS\tUNMAPPED_READS\tUNPAIRED_READ_DUPLICATES\tREAD_PAIR_DUPLICATES\tREAD_PAIR_OPTICAL_DUPLICATES\tPERCENT_DUPLICATION\tESTIMATED_LIBRARY_SIZE\n",
            "Unknown Library\t{}\t0\t0\t{}\t{}\t0\t0\t{:.6}\t{}\n"
        ),
        summary.records_examined,
        summary.unmapped_records,
        summary.duplicate_records,
        percent_duplication,
        summary
            .records_examined
            .saturating_sub(summary.duplicate_records)
    )
}
