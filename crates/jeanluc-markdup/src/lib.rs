#![forbid(unsafe_code)]

use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use rust_htslib::bam::record::Cigar;
use rust_htslib::bam::{self, Read, index};
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
    pub secondary_or_supplementary_records: u64,
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
        secondary_or_supplementary_records: 0,
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
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
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
    let mut records = Vec::new();
    let mut eligible_indices = Vec::new();
    let mut summary = MarkDuplicatesSummary {
        library,
        unpaired_reads_examined: 0,
        read_pairs_examined: 0,
        secondary_or_supplementary_records: 0,
        unpaired_duplicate_records: 0,
        duplicate_pair_records: 0,
        unmapped_records: 0,
    };

    for result in reader.records() {
        let record = result?;
        let flag = record.flags();
        let record_index = records.len();

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            records.push(record);
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            records.push(record);
            continue;
        }

        if flag & PAIRED_FLAG != 0 {
            if flag & FIRST_IN_PAIR_FLAG != 0 {
                summary.read_pairs_examined += 1;
            }
        } else {
            summary.unpaired_reads_examined += 1;
        }
        eligible_indices.push(record_index);
        records.push(record);
    }

    let duplicate_groups = duplicate_groups(&records, &eligible_indices);

    for group in duplicate_groups.values() {
        if group.len() < 2 {
            continue;
        }

        let representative_name = best_duplicate_representative_name(group, &records);

        for index in group.iter().copied() {
            if query_name(&records[index]) == representative_name {
                continue;
            }
            let flag = records[index].flags();
            if flag & PAIRED_FLAG != 0 {
                summary.duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
            }
            records[index].set_flags(flag | DUPLICATE_FLAG);
        }
    }

    {
        for mut record in records {
            if config.remove_duplicates && record.flags() & DUPLICATE_FLAG != 0 {
                continue;
            }
            if config.clear_dt {
                clear_duplicate_type_tag(&mut record)?;
            }
            writer.write(&record)?;
        }
    }
    drop(writer);

    fs::write(&config.metrics_file, metrics_text(&summary))?;
    if config.create_md5_file {
        write_md5_sidecar(&config.output)?;
    }
    if config.create_index {
        index::build(
            &config.output,
            Some(&picard_bai_path(&config.output)),
            index::Type::Bai,
            1,
        )?;
    }
    Ok(summary)
}

fn clear_duplicate_type_tag(record: &mut bam::Record) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"DT").is_ok() {
        record.remove_aux(b"DT")?;
    }
    Ok(())
}

fn picard_bai_path(output: &str) -> String {
    Path::new(output)
        .with_extension("bai")
        .display()
        .to_string()
}

fn write_md5_sidecar(output: &str) -> Result<(), MarkDuplicatesError> {
    let bytes = fs::read(output)?;
    let digest = md5::compute(bytes);
    fs::write(format!("{output}.md5"), format!("{digest:x}"))?;
    Ok(())
}

fn duplicate_key(fields: &[String], flag: u16) -> DuplicateKey {
    let reverse_strand = flag & 0x10 != 0;
    let position = fields[3].parse::<i64>().unwrap_or_default() - 1;
    DuplicateKey {
        reference_name: fields[2].clone(),
        position: unclipped_five_prime_position(position, &fields[5], reverse_strand),
        mate_reference_name: fields[6].clone(),
        mate_position: fields[7].parse::<i64>().unwrap_or_default(),
        template_length: fields[8].parse::<i64>().unwrap_or_default(),
        reverse_strand,
    }
}

fn duplicate_groups(
    records: &[bam::Record],
    eligible_indices: &[usize],
) -> HashMap<DuplicateKey, Vec<usize>> {
    let mut paired_by_name = HashMap::<String, Vec<usize>>::new();
    let mut duplicate_groups = HashMap::<DuplicateKey, Vec<usize>>::new();

    for index in eligible_indices.iter().copied() {
        let record = &records[index];
        if record.flags() & PAIRED_FLAG != 0 {
            paired_by_name
                .entry(query_name(record))
                .or_default()
                .push(index);
        } else {
            duplicate_groups
                .entry(single_duplicate_key_bam(record))
                .or_default()
                .push(index);
        }
    }

    for indices in paired_by_name.into_values() {
        let key = if indices.len() >= 2 {
            pair_duplicate_key_bam(&records[indices[0]], &records[indices[1]])
        } else {
            single_duplicate_key_bam(&records[indices[0]])
        };
        duplicate_groups.entry(key).or_default().extend(indices);
    }

    duplicate_groups
}

fn single_duplicate_key_bam(record: &bam::Record) -> DuplicateKey {
    let reverse_strand = record.flags() & 0x10 != 0;
    let position = unclipped_record_position(record);
    DuplicateKey {
        reference_name: record.tid().to_string(),
        position,
        mate_reference_name: record.mtid().to_string(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        reverse_strand,
    }
}

fn pair_duplicate_key_bam(first: &bam::Record, second: &bam::Record) -> DuplicateKey {
    let first_position = unclipped_record_position(first);
    let second_position = unclipped_record_position(second);
    let (left, right) = if (first.tid(), first_position) <= (second.tid(), second_position) {
        (first, second)
    } else {
        (second, first)
    };

    DuplicateKey {
        reference_name: left.tid().to_string(),
        position: unclipped_record_position(left),
        mate_reference_name: right.tid().to_string(),
        mate_position: unclipped_record_position(right),
        template_length: first.insert_size().abs().max(second.insert_size().abs()),
        reverse_strand: false,
    }
}

fn unclipped_record_position(record: &bam::Record) -> i64 {
    let reverse_strand = record.flags() & 0x10 != 0;
    let cigar = record.cigar();
    if reverse_strand {
        let reference_len: i64 = cigar
            .iter()
            .filter(|operation| consumes_reference(operation))
            .map(cigar_len)
            .sum();
        let trailing_clip: i64 = cigar
            .iter()
            .rev()
            .take_while(|operation| is_clip(operation))
            .map(cigar_len)
            .sum();
        record.pos() + reference_len + trailing_clip - 1
    } else {
        let leading_clip: i64 = cigar
            .iter()
            .take_while(|operation| is_clip(operation))
            .map(cigar_len)
            .sum();
        record.pos() - leading_clip
    }
}

fn consumes_reference(operation: &Cigar) -> bool {
    matches!(
        operation,
        Cigar::Match(_) | Cigar::Del(_) | Cigar::RefSkip(_) | Cigar::Equal(_) | Cigar::Diff(_)
    )
}

fn is_clip(operation: &Cigar) -> bool {
    matches!(operation, Cigar::SoftClip(_) | Cigar::HardClip(_))
}

fn cigar_len(operation: &Cigar) -> i64 {
    match operation {
        Cigar::Match(len)
        | Cigar::Ins(len)
        | Cigar::Del(len)
        | Cigar::RefSkip(len)
        | Cigar::SoftClip(len)
        | Cigar::HardClip(len)
        | Cigar::Pad(len)
        | Cigar::Equal(len)
        | Cigar::Diff(len) => i64::from(*len),
    }
}

fn unclipped_five_prime_position(position: i64, cigar: &str, reverse_strand: bool) -> i64 {
    let operations = parse_cigar(cigar);
    if reverse_strand {
        let reference_len: i64 = operations
            .iter()
            .filter(|(_, op)| matches!(op, 'M' | 'D' | 'N' | '=' | 'X'))
            .map(|(len, _)| *len)
            .sum();
        let trailing_clip = operations
            .iter()
            .rev()
            .take_while(|(_, op)| matches!(op, 'S' | 'H'))
            .map(|(len, _)| *len)
            .sum::<i64>();
        position + reference_len + trailing_clip - 1
    } else {
        let leading_clip = operations
            .iter()
            .take_while(|(_, op)| matches!(op, 'S' | 'H'))
            .map(|(len, _)| *len)
            .sum::<i64>();
        position - leading_clip
    }
}

fn parse_cigar(cigar: &str) -> Vec<(i64, char)> {
    let mut operations = Vec::new();
    let mut length = String::new();

    for character in cigar.chars() {
        if character.is_ascii_digit() {
            length.push(character);
            continue;
        }
        if !length.is_empty() {
            operations.push((length.parse::<i64>().unwrap_or_default(), character));
            length.clear();
        }
    }

    operations
}

fn quality_score(record: &bam::Record) -> u64 {
    record
        .qual()
        .iter()
        .map(|quality| u64::from(*quality))
        .sum()
}

fn best_duplicate_representative_name(group: &[usize], records: &[bam::Record]) -> String {
    let mut scores = Vec::<(String, u64, usize)>::new();

    for index in group.iter().copied() {
        let name = query_name(&records[index]);
        if let Some((_, score, _)) = scores
            .iter_mut()
            .find(|(existing_name, _, _)| existing_name == &name)
        {
            *score += quality_score(&records[index]);
        } else {
            scores.push((name, quality_score(&records[index]), index));
        }
    }

    scores
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.2.cmp(&left.2)))
        .map(|(name, _, _)| name)
        .expect("non-empty duplicate group")
}

fn query_name(record: &bam::Record) -> String {
    String::from_utf8_lossy(record.qname()).into_owned()
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
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t{:.6}\t{}\n"
        ),
        summary.library,
        summary.unpaired_reads_examined,
        summary.read_pairs_examined,
        summary.secondary_or_supplementary_records,
        summary.unmapped_records,
        summary.unpaired_duplicate_records,
        summary.read_pair_duplicates(),
        percent_duplication,
        estimated_library_size
    )
}

const PAIRED_FLAG: u16 = 0x1;
const FIRST_IN_PAIR_FLAG: u16 = 0x40;
const SECONDARY_OR_SUPPLEMENTARY_FLAGS: u16 = 0x100 | 0x800;

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
