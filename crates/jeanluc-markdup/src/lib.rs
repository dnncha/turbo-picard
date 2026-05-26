#![forbid(unsafe_code)]

use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::record::{Aux, Cigar};
use rust_htslib::bam::{self, Read, index};
use std::collections::{HashMap, HashSet};
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
    pub read_pair_optical_duplicates: u64,
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
    barcode: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BamDuplicateKey {
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    template_length: i64,
    reverse_strand: bool,
    barcode: Option<Vec<u8>>,
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
        read_pair_optical_duplicates: 0,
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
        let key = duplicate_key(&fields, flag, config);
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
            if config.tagging_policy.as_deref() == Some("All") && flag & DUPLICATE_FLAG != 0 {
                add_duplicate_type_tag_to_sam_fields(&mut fields);
            }
            if config.add_pg_tag_to_reads {
                add_program_group_to_sam_fields(&mut fields);
            }
            output.push_str(&fields.join("\t"));
            output.push('\n');
        }
    }

    if config.add_pg_tag_to_reads {
        add_program_group_to_sam_header(&mut output);
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
    let mut header = bam::Header::from_template(reader.header());
    if config.add_pg_tag_to_reads {
        header.push_record(
            HeaderRecord::new(b"PG")
                .push_tag(b"ID", "MarkDuplicates")
                .push_tag(b"PN", "MarkDuplicates"),
        );
    }
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
        read_pair_optical_duplicates: 0,
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

    let duplicate_groups = duplicate_groups(&records, &eligible_indices, config);
    let mut optical_duplicate_name_set = HashSet::<Vec<u8>>::new();

    for group in duplicate_groups.values() {
        if group.len() < 2 {
            continue;
        }

        let representative_name = best_duplicate_representative_name(group, &records);
        let optical_names =
            optical_duplicate_names(group, &records, representative_name.as_slice(), config);
        summary.read_pair_optical_duplicates += optical_names.len() as u64;
        optical_duplicate_name_set.extend(optical_names);
        if config.tag_duplicate_set_members && !config.remove_duplicates {
            add_duplicate_set_member_tags(group, &mut records, representative_name.as_slice())?;
        }

        for index in group.iter().copied() {
            if records[index].qname() == representative_name.as_slice() {
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
            if let Some(duplicate_type) = duplicate_type_tag(
                config,
                record.flags(),
                record.qname(),
                &optical_duplicate_name_set,
            ) {
                add_duplicate_type_tag(&mut record, duplicate_type)?;
            }
            if config.add_pg_tag_to_reads {
                add_program_group_to_bam_record(&mut record)?;
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

fn duplicate_type_tag<'a>(
    config: &'a MarkDuplicatesConfig,
    flags: u16,
    name: &[u8],
    optical_duplicate_names: &HashSet<Vec<u8>>,
) -> Option<&'a str> {
    if flags & DUPLICATE_FLAG == 0 {
        return None;
    }
    if optical_duplicate_names.contains(name) {
        return match config.tagging_policy.as_deref() {
            Some("All" | "OpticalOnly") => Some("SQ"),
            _ => None,
        };
    }
    if config.tagging_policy.as_deref() == Some("All") {
        Some("LB")
    } else {
        None
    }
}

fn add_duplicate_type_tag(
    record: &mut bam::Record,
    duplicate_type: &str,
) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"DT").is_ok() {
        record.remove_aux(b"DT")?;
    }
    record.push_aux(b"DT", Aux::String(duplicate_type))?;
    Ok(())
}

fn optical_duplicate_names(
    group: &[usize],
    records: &[bam::Record],
    representative_name: &[u8],
    config: &MarkDuplicatesConfig,
) -> Vec<Vec<u8>> {
    if config.read_name_regex.as_deref() == Some("null") {
        return Vec::new();
    }
    let Some(representative_location) = read_location_for_name(group, records, representative_name)
    else {
        return Vec::new();
    };
    let pixel_distance = i64::from(config.optical_duplicate_pixel_distance.unwrap_or(100));
    let mut optical_names = Vec::<Vec<u8>>::new();

    for index in group.iter().copied() {
        let name = records[index].qname();
        if name == representative_name || optical_names.iter().any(|existing| existing == name) {
            continue;
        }
        let Some(location) = parse_read_location(name) else {
            continue;
        };
        if representative_location.is_within(&location, pixel_distance) {
            optical_names.push(name.to_vec());
        }
    }

    optical_names
}

fn read_location_for_name(
    group: &[usize],
    records: &[bam::Record],
    name: &[u8],
) -> Option<ReadLocation> {
    group
        .iter()
        .find(|index| records[**index].qname() == name)
        .and_then(|index| parse_read_location(records[*index].qname()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadLocation {
    tile: i64,
    x: i64,
    y: i64,
}

impl ReadLocation {
    fn is_within(&self, other: &Self, pixel_distance: i64) -> bool {
        self.tile == other.tile
            && (self.x - other.x).abs() <= pixel_distance
            && (self.y - other.y).abs() <= pixel_distance
    }
}

fn parse_read_location(name: &[u8]) -> Option<ReadLocation> {
    let text = std::str::from_utf8(name).ok()?;
    let mut parts = text.rsplit(':');
    let y = parts.next()?.parse::<i64>().ok()?;
    let x = parts.next()?.parse::<i64>().ok()?;
    let tile = parts.next()?.parse::<i64>().ok()?;
    Some(ReadLocation { tile, x, y })
}

fn add_duplicate_set_member_tags(
    group: &[usize],
    records: &mut [bam::Record],
    representative_name: &[u8],
) -> Result<(), MarkDuplicatesError> {
    if !group
        .iter()
        .any(|index| records[*index].flags() & PAIRED_FLAG != 0)
    {
        return Ok(());
    }

    let mut member_names = Vec::<Vec<u8>>::new();
    for index in group.iter().copied() {
        let name = records[index].qname().to_vec();
        if !member_names.iter().any(|existing| existing == &name) {
            member_names.push(name);
        }
    }
    if member_names.len() < 2 {
        return Ok(());
    }

    let duplicate_set_index = group
        .iter()
        .copied()
        .filter(|index| records[*index].qname() == representative_name)
        .min()
        .unwrap_or(group[0]);
    let duplicate_set_size = i32::try_from(member_names.len()).unwrap_or(i32::MAX);
    let duplicate_set_index = i32::try_from(duplicate_set_index).unwrap_or(i32::MAX);

    for index in group.iter().copied() {
        replace_i32_aux(&mut records[index], b"DS", duplicate_set_size)?;
        replace_i32_aux(&mut records[index], b"DI", duplicate_set_index)?;
    }

    Ok(())
}

fn replace_i32_aux(
    record: &mut bam::Record,
    tag: &[u8],
    value: i32,
) -> Result<(), MarkDuplicatesError> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag)?;
    }
    record.push_aux(tag, Aux::I32(value))?;
    Ok(())
}

fn add_duplicate_type_tag_to_sam_fields(fields: &mut Vec<String>) {
    fields.retain(|field| !field.starts_with("DT:Z:"));
    fields.push("DT:Z:LB".to_string());
}

fn add_program_group_to_bam_record(record: &mut bam::Record) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"PG").is_ok() {
        record.remove_aux(b"PG")?;
    }
    record.push_aux(b"PG", Aux::String("MarkDuplicates"))?;
    Ok(())
}

fn add_program_group_to_sam_fields(fields: &mut Vec<String>) {
    fields.retain(|field| !field.starts_with("PG:Z:"));
    fields.push("PG:Z:MarkDuplicates".to_string());
}

fn add_program_group_to_sam_header(output: &mut String) {
    let insert_at = output
        .lines()
        .take_while(|line| line.starts_with('@'))
        .map(|line| line.len() + 1)
        .sum::<usize>();
    output.insert_str(insert_at, "@PG\tID:MarkDuplicates\tPN:MarkDuplicates\n");
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

fn duplicate_key(fields: &[String], flag: u16, config: &MarkDuplicatesConfig) -> DuplicateKey {
    let reverse_strand = flag & 0x10 != 0;
    let position = fields[3].parse::<i64>().unwrap_or_default() - 1;
    DuplicateKey {
        reference_name: fields[2].clone(),
        position: unclipped_five_prime_position(position, &fields[5], reverse_strand),
        mate_reference_name: fields[6].clone(),
        mate_position: fields[7].parse::<i64>().unwrap_or_default(),
        template_length: fields[8].parse::<i64>().unwrap_or_default(),
        reverse_strand,
        barcode: sam_barcode(fields, config),
    }
}

fn duplicate_groups(
    records: &[bam::Record],
    eligible_indices: &[usize],
    config: &MarkDuplicatesConfig,
) -> HashMap<BamDuplicateKey, Vec<usize>> {
    let mut paired_by_name = HashMap::<Vec<u8>, Vec<usize>>::new();
    let mut duplicate_groups = HashMap::<BamDuplicateKey, Vec<usize>>::new();

    for index in eligible_indices.iter().copied() {
        let record = &records[index];
        if record.flags() & PAIRED_FLAG != 0 {
            paired_by_name
                .entry(record.qname().to_vec())
                .or_default()
                .push(index);
        } else {
            duplicate_groups
                .entry(single_duplicate_key_bam(
                    record,
                    &bam_barcode(record, config),
                ))
                .or_default()
                .push(index);
        }
    }

    for indices in paired_by_name.into_values() {
        let barcode = first_barcode(records, &indices, config);
        let key = if indices.len() >= 2 {
            pair_duplicate_key_bam(&records[indices[0]], &records[indices[1]], barcode)
        } else {
            single_duplicate_key_bam(&records[indices[0]], &barcode)
        };
        duplicate_groups.entry(key).or_default().extend(indices);
    }

    duplicate_groups
}

fn sam_barcode(fields: &[String], config: &MarkDuplicatesConfig) -> Option<Vec<u8>> {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return sam_tag_value(fields, tag);
    }

    combined_barcode(
        config
            .read_one_barcode_tag
            .as_deref()
            .and_then(|tag| sam_tag_value(fields, tag)),
        config
            .read_two_barcode_tag
            .as_deref()
            .and_then(|tag| sam_tag_value(fields, tag)),
    )
}

fn sam_tag_value(fields: &[String], tag: &str) -> Option<Vec<u8>> {
    let prefix = format!("{tag}:Z:");
    fields.iter().skip(11).find_map(|field| {
        field
            .strip_prefix(&prefix)
            .map(|value| value.as_bytes().to_vec())
    })
}

fn first_barcode(
    records: &[bam::Record],
    indices: &[usize],
    config: &MarkDuplicatesConfig,
) -> Option<Vec<u8>> {
    indices
        .iter()
        .find_map(|index| bam_barcode(&records[*index], config))
}

fn bam_barcode(record: &bam::Record, config: &MarkDuplicatesConfig) -> Option<Vec<u8>> {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return bam_tag_value(record, tag);
    }

    combined_barcode(
        config
            .read_one_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
        config
            .read_two_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
    )
}

fn bam_tag_value(record: &bam::Record, tag: &str) -> Option<Vec<u8>> {
    match record.aux(tag.as_bytes()) {
        Ok(Aux::String(value)) => Some(value.as_bytes().to_vec()),
        Ok(Aux::Char(value)) => Some(vec![value]),
        _ => None,
    }
}

fn combined_barcode<T>(read_one: Option<T>, read_two: Option<T>) -> Option<Vec<u8>>
where
    T: AsRef<[u8]>,
{
    match (read_one, read_two) {
        (None, None) => None,
        (read_one, read_two) => {
            let mut barcode = Vec::new();
            if let Some(value) = read_one {
                barcode.extend_from_slice(value.as_ref());
            }
            barcode.push(b'|');
            if let Some(value) = read_two {
                barcode.extend_from_slice(value.as_ref());
            }
            Some(barcode)
        }
    }
}

fn single_duplicate_key_bam(record: &bam::Record, barcode: &Option<Vec<u8>>) -> BamDuplicateKey {
    let reverse_strand = record.flags() & 0x10 != 0;
    let position = unclipped_record_position(record);
    BamDuplicateKey {
        reference_id: record.tid(),
        position,
        mate_reference_id: record.mtid(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        reverse_strand,
        barcode: barcode.clone(),
    }
}

fn pair_duplicate_key_bam(
    first: &bam::Record,
    second: &bam::Record,
    barcode: Option<Vec<u8>>,
) -> BamDuplicateKey {
    let first_position = unclipped_record_position(first);
    let second_position = unclipped_record_position(second);
    let (left, right) = if (first.tid(), first_position) <= (second.tid(), second_position) {
        (first, second)
    } else {
        (second, first)
    };

    BamDuplicateKey {
        reference_id: left.tid(),
        position: unclipped_record_position(left),
        mate_reference_id: right.tid(),
        mate_position: unclipped_record_position(right),
        template_length: first.insert_size().abs().max(second.insert_size().abs()),
        reverse_strand: false,
        barcode,
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

fn best_duplicate_representative_name(group: &[usize], records: &[bam::Record]) -> Vec<u8> {
    let mut scores = Vec::<(Vec<u8>, u64, usize)>::new();

    for index in group.iter().copied() {
        let name = records[index].qname().to_vec();
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

fn metrics_text(summary: &MarkDuplicatesSummary) -> String {
    let duplicate_fragments =
        summary.unpaired_duplicate_records + (summary.read_pair_duplicates() * 2);
    let examined_fragments = summary.unpaired_reads_examined + (summary.read_pairs_examined * 2);
    let percent_duplication = if examined_fragments == 0 {
        0.0
    } else {
        duplicate_fragments as f64 / examined_fragments as f64
    };
    let estimated_library_size =
        if summary.read_pairs_examined > 0 && summary.read_pair_optical_duplicates == 0 {
            summary.read_pairs_examined.to_string()
        } else {
            String::new()
        };

    format!(
        concat!(
            "## METRICS CLASS\tpicard.sam.DuplicationMetrics\n",
            "LIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\tSECONDARY_OR_SUPPLEMENTARY_RDS\tUNMAPPED_READS\tUNPAIRED_READ_DUPLICATES\tREAD_PAIR_DUPLICATES\tREAD_PAIR_OPTICAL_DUPLICATES\tPERCENT_DUPLICATION\tESTIMATED_LIBRARY_SIZE\n",
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n"
        ),
        summary.library,
        summary.unpaired_reads_examined,
        summary.read_pairs_examined,
        summary.secondary_or_supplementary_records,
        summary.unmapped_records,
        summary.unpaired_duplicate_records,
        summary.read_pair_duplicates(),
        summary.read_pair_optical_duplicates,
        format_metric_float(percent_duplication),
        estimated_library_size
    )
}

fn format_metric_float(value: f64) -> String {
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
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
