#![forbid(unsafe_code)]

use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use rust_htslib::bam::{self, Read};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;

const DUPLICATE_FLAG: u16 = 0x400;
const UNMAPPED_FLAG: u16 = 0x4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesSummary {
    pub library: String,
    pub unpaired_reads_examined: u64,
    pub read_pairs_examined: u64,
    pub unpaired_duplicate_records: u64,
    pub duplicate_pair_records: u64,
    pub unmapped_records: u64,
}

#[derive(Debug)]
pub enum MarkDuplicatesError {
    UnsupportedInputFormat(String),
    Io(std::io::Error),
    Htslib(rust_htslib::errors::Error),
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
            Self::Htslib(error) => write!(f, "{error}"),
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

impl From<rust_htslib::errors::Error> for MarkDuplicatesError {
    fn from(value: rust_htslib::errors::Error) -> Self {
        Self::Htslib(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DuplicateKey {
    reference_name: String,
    position: i64,
    cigar: String,
    mate_reference_name: String,
    mate_position: i64,
    template_length: i64,
    reverse_strand: bool,
}

pub fn run(config: &MarkDuplicatesConfig) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    if is_bam_input(&config.input) {
        return run_bam(config);
    }

    ensure_sam_input(&config.input)?;

    let input = fs::read_to_string(&config.input)?;
    let mut seen = HashMap::<DuplicateKey, usize>::new();
    let mut output = String::with_capacity(input.len());
    let mut summary = MarkDuplicatesSummary {
        library: "Unknown Library".to_string(),
        unpaired_reads_examined: 0,
        read_pairs_examined: 0,
        unpaired_duplicate_records: 0,
        duplicate_pair_records: 0,
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

        if flag & PAIRED_FLAG != 0 {
            if flag & FIRST_IN_PAIR_FLAG != 0 {
                summary.read_pairs_examined += 1;
            }
        } else {
            summary.unpaired_reads_examined += 1;
        }
        let key = duplicate_key(&fields, flag);
        let seen_count = seen.entry(key).or_insert(0);
        let duplicate = *seen_count > 0;
        *seen_count += 1;

        if duplicate {
            if flag & PAIRED_FLAG != 0 {
                summary.duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
            }
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

fn is_bam_input(input: &str) -> bool {
    Path::new(input)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bam"))
}

fn run_bam(config: &MarkDuplicatesConfig) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    let mut reader = bam::Reader::from_path(&config.input)?;
    let library = first_library_name(reader.header());
    let header = bam::Header::from_template(reader.header());
    let mut writer = bam::Writer::from_path(&config.output, &header, bam::Format::Bam)?;
    let mut seen = HashMap::<DuplicateKey, usize>::new();
    let mut summary = MarkDuplicatesSummary {
        library,
        unpaired_reads_examined: 0,
        read_pairs_examined: 0,
        unpaired_duplicate_records: 0,
        duplicate_pair_records: 0,
        unmapped_records: 0,
    };

    for result in reader.records() {
        let mut record = result?;
        let mut flag = record.flags();

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            writer.write(&record)?;
            continue;
        }

        if flag & PAIRED_FLAG != 0 {
            if flag & FIRST_IN_PAIR_FLAG != 0 {
                summary.read_pairs_examined += 1;
            }
        } else {
            summary.unpaired_reads_examined += 1;
        }
        let key = duplicate_key_bam(&record);
        let seen_count = seen.entry(key).or_insert(0);
        let duplicate = *seen_count > 0;
        *seen_count += 1;

        if duplicate {
            if flag & PAIRED_FLAG != 0 {
                summary.duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
            }
            flag |= DUPLICATE_FLAG;
            record.set_flags(flag);
        }

        if !(duplicate && config.remove_duplicates) {
            writer.write(&record)?;
        }
    }

    fs::write(&config.metrics_file, metrics_text(&summary))?;
    Ok(summary)
}

fn duplicate_key(fields: &[String], flag: u16) -> DuplicateKey {
    DuplicateKey {
        reference_name: fields[2].clone(),
        position: fields[3].parse::<i64>().unwrap_or_default(),
        cigar: fields[5].clone(),
        mate_reference_name: fields[6].clone(),
        mate_position: fields[7].parse::<i64>().unwrap_or_default(),
        template_length: fields[8].parse::<i64>().unwrap_or_default(),
        reverse_strand: flag & 0x10 != 0,
    }
}

fn duplicate_key_bam(record: &bam::Record) -> DuplicateKey {
    DuplicateKey {
        reference_name: record.tid().to_string(),
        position: record.pos(),
        cigar: record.cigar().to_string(),
        mate_reference_name: record.mtid().to_string(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        reverse_strand: record.flags() & 0x10 != 0,
    }
}

fn metrics_text(summary: &MarkDuplicatesSummary) -> String {
    let duplicate_fragments =
        summary.unpaired_duplicate_records + (summary.read_pair_duplicates() * 2);
    let examined_fragments = summary.unpaired_reads_examined + (summary.read_pairs_examined * 2);
    let percent_duplication = if examined_fragments == 0 {
        0.0
    } else {
        duplicate_fragments as f64 / examined_fragments as f64
    };
    let estimated_library_size = if summary.read_pairs_examined > 0 {
        summary.read_pairs_examined.to_string()
    } else {
        String::new()
    };

    format!(
        concat!(
            "## METRICS CLASS\tpicard.sam.DuplicationMetrics\n",
            "LIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\tSECONDARY_OR_SUPPLEMENTARY_RDS\tUNMAPPED_READS\tUNPAIRED_READ_DUPLICATES\tREAD_PAIR_DUPLICATES\tREAD_PAIR_OPTICAL_DUPLICATES\tPERCENT_DUPLICATION\tESTIMATED_LIBRARY_SIZE\n",
            "{}\t{}\t{}\t0\t{}\t{}\t{}\t0\t{:.6}\t{}\n"
        ),
        summary.library,
        summary.unpaired_reads_examined,
        summary.read_pairs_examined,
        summary.unmapped_records,
        summary.unpaired_duplicate_records,
        summary.read_pair_duplicates(),
        percent_duplication,
        estimated_library_size
    )
}

const PAIRED_FLAG: u16 = 0x1;
const FIRST_IN_PAIR_FLAG: u16 = 0x40;

impl MarkDuplicatesSummary {
    fn read_pair_duplicates(&self) -> u64 {
        self.duplicate_pair_records / 2
    }
}

fn first_library_name(header: &bam::HeaderView) -> String {
    let header_text = String::from_utf8_lossy(header.as_bytes());
    for line in header_text.lines() {
        if !line.starts_with("@RG\t") {
            continue;
        }
        for field in line.split('\t') {
            if let Some(library) = field.strip_prefix("LB:") {
                return library.to_string();
            }
        }
    }

    "Unknown Library".to_string()
}
