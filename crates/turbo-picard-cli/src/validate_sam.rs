use crate::command_error::{CommandError, unsupported};
use crate::{
    aux_i32, dna_bases_equal, has_sam_extension, i32_or_max, open_bam_reader_with_reference,
    optional_bool, optional_scalar, optional_u32, parse_i64_bytes, parse_u16_bytes,
    picard_reference, read_group_id, reference_sequences_by_name,
    reject_unsupported_compression_level, required_scalar_for, sam_structure, u32_or_max,
    u64_or_max, usize_or_max,
};
use rust_htslib::bam::record::{Aux, Cigar};
use rust_htslib::bam::{self, Read};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use turbo_picard_core::picard_args::{PicardArgs, normalize_picard_args_for_command};

pub(crate) const VALIDATE_SAM_FILE_VALIDATION_ISSUES: &str =
    "ValidateSamFile found validation issues";

pub(crate) enum ValidateSamFileStatus {
    Valid,
    Invalid { exit_code: i32 },
}

fn validatesamfile_validation_issue_exit_code(counts: &BTreeMap<String, u64>) -> i32 {
    match (
        counts.keys().any(|key| key.starts_with("ERROR:")),
        counts.keys().any(|key| key.starts_with("WARNING:")),
    ) {
        (false, false) => 0,
        (false, true) => 1,
        (true, true) => 2,
        (true, false) => 3,
    }
}

pub(crate) fn run_validatesamfile(args: &[String]) -> Result<ValidateSamFileStatus, CommandError> {
    let args = normalize_picard_args_for_command("ValidateSamFile", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_validatesamfile_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "ValidateSamFile")?;
    let output = optional_scalar(&args, "OUTPUT")?;
    let skip_mate_validation = optional_bool(&args, "SKIP_MATE_VALIDATION")?.unwrap_or(false);
    let ignored = validate_sam_ignored_summary_keys(&args)?;
    let mode = validate_sam_mode(&args)?;
    let max_output = optional_u32(&args, "MAX_OUTPUT")?;
    let reference = picard_reference(&args)?;
    if let Some(reference_path) = reference.as_deref() {
        fs::metadata(reference_path).map_err(|_| {
            format!("ValidateSamFile reference sequence {reference_path} does not exist")
        })?;
    }

    let reference_by_name = match reference.as_deref() {
        Some(reference_path) => Some(reference_sequences_by_name(reference_path)?),
        None => None,
    };

    let mut report = if has_sam_extension(&input)
        && mode == ValidateSamMode::Summary
        && reference_by_name.is_none()
    {
        validate_sam_summary_sam_text(&input, skip_mate_validation)?
    } else {
        let mut reader = open_bam_reader_for_validate_sam_file(&input, reference.as_deref())
            .map_err(|error| error.clone())?;
        validate_sam_summary(
            &mut reader,
            skip_mate_validation,
            reference_by_name.as_ref(),
        )?
    };
    for key in ignored {
        report.counts.remove(&key);
        report.details.retain(|detail| detail.key != key);
    }
    match mode {
        ValidateSamMode::Summary => write_validate_sam_summary(output.as_deref(), &report.counts)?,
        ValidateSamMode::Verbose => {
            write_validate_sam_verbose(output.as_deref(), &report.details, max_output)?;
        }
    }

    if report.counts.is_empty() {
        Ok(ValidateSamFileStatus::Valid)
    } else {
        let exit_code = validatesamfile_validation_issue_exit_code(&report.counts);
        Ok(ValidateSamFileStatus::Invalid { exit_code })
    }
}

fn reject_unsupported_validatesamfile_args(args: &PicardArgs) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "MODE",
        "MAX_OUTPUT",
        "IGNORE",
        "SKIP_MATE_VALIDATION",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "COMPRESSION_LEVEL",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported ValidateSamFile argument: {key}"
            )));
        }
    }
    validate_sam_mode(args)?;
    optional_u32(args, "MAX_OUTPUT")?;
    optional_bool(args, "SKIP_MATE_VALIDATION")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    reject_unsupported_compression_level(args, "ValidateSamFile")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    validate_sam_ignored_summary_keys(args)?;
    Ok(())
}

#[derive(Debug, Default)]
struct ValidateSamReport {
    counts: BTreeMap<String, u64>,
    details: Vec<ValidateSamDetail>,
}

#[derive(Debug)]
struct ValidateSamDetail {
    key: String,
    line: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValidateSamMode {
    Summary,
    Verbose,
}

fn validate_sam_summary(
    reader: &mut bam::Reader,
    skip_mate_validation: bool,
    reference_by_name: Option<&BTreeMap<String, Vec<u8>>>,
) -> Result<ValidateSamReport, CommandError> {
    let header_text = String::from_utf8_lossy(reader.header().as_bytes()).to_string();
    let read_groups = read_group_platforms(&header_text);
    let mut structure = sam_structure::Structure::default();
    for line in header_text.lines() {
        structure.header(line)?;
    }
    let target_count = reader.header().target_count();
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>();
    let references_by_tid = reference_by_name.map(|references| {
        target_names
            .iter()
            .map(|name| references.get(name).map(Vec::as_slice))
            .collect::<Vec<_>>()
    });
    let mut report = ValidateSamReport::default();
    let mut pending_mates = BTreeMap::<Vec<u8>, ValidateSamMate>::new();

    if target_count == 0 {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "ERROR::MISSING_SEQUENCE_DICTIONARY:Sequence dictionary is empty".to_string(),
        );
    }
    if read_groups.is_empty() {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_READ_GROUP",
            "ERROR::MISSING_READ_GROUP:Read groups is empty".to_string(),
        );
    } else {
        for has_platform in read_groups.values() {
            if !has_platform {
                add_validate_issue(
                    &mut report,
                    "ERROR:MISSING_PLATFORM_VALUE",
                    "ERROR::MISSING_PLATFORM_VALUE:A platform value is missing".to_string(),
                );
            }
        }
    }

    let mut record_number = 0_u64;
    let mut record = bam::Record::new();
    while let Some(result) = reader.read(&mut record) {
        result.map_err(|error| error.to_string())?;
        record_number += 1;
        let reference = if record.tid() < 0 {
            "*"
        } else {
            target_names
                .get(usize_or_max(record.tid()))
                .map_or("*", String::as_str)
        };
        let quality_length = (record.qual().first() != Some(&255)).then_some(record.qual().len());
        for issue in structure.observe(
            record.flags(),
            reference,
            u64_or_max(record.pos().saturating_add(1).max(0)),
            &record.cigar().to_string(),
            record.seq_len(),
            quality_length,
        )? {
            add_validate_issue(
                &mut report,
                issue,
                format!(
                    "{}:Record {record_number}, Read name {}",
                    issue.replace(':', "::"),
                    validate_qname(&record)
                ),
            );
        }
        if record.is_paired() && !skip_mate_validation {
            validate_sam_mate_summary(&record, &mut pending_mates, &mut report);
        }
        validate_sam_record_summary(
            &record,
            record_number,
            target_count,
            &read_groups,
            references_by_tid.as_deref(),
            &mut report,
        )?;
    }
    for _ in pending_mates.values() {
        add_validate_issue(
            &mut report,
            "ERROR:MATE_NOT_FOUND",
            "ERROR::MATE_NOT_FOUND:Mate not found for paired read".to_string(),
        );
    }

    Ok(report)
}

fn validate_sam_summary_sam_text(
    input: &str,
    skip_mate_validation: bool,
) -> Result<ValidateSamReport, CommandError> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut target_count = 0_u32;
    let mut structure = sam_structure::Structure::default();
    let mut report = ValidateSamReport::default();
    let mut pending_mates = BTreeMap::<Vec<u8>, ValidateSamMate>::new();
    let mut read_groups = BTreeMap::<String, bool>::new();
    let mut record_number = 0_u64;

    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") {
            let text = String::from_utf8_lossy(&line);
            structure.header(&text)?;
            if line.starts_with(b"@SQ\t") {
                target_count += 1;
            }
            if line.starts_with(b"@RG\t")
                && let Some(id) = read_group_id(&text)
            {
                let has_platform = text.split('\t').any(|field| {
                    field
                        .strip_prefix("PL:")
                        .is_some_and(|value| !value.trim_end_matches(['\r', '\n']).is_empty())
                });
                read_groups.insert(id, has_platform);
            }
            continue;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }

        record_number += 1;
        let flags = validate_sam_record_summary_sam_line(
            &line,
            record_number,
            target_count,
            &read_groups,
            &mut report,
        )?;
        let text = std::str::from_utf8(&line).map_err(|error| error.to_string())?;
        let fields: Vec<_> = text.trim_end_matches(['\r', '\n']).split('\t').collect();
        if fields.len() < 11 {
            return Err("malformed ValidateSamFile SAM record".into());
        }
        let position = fields[3]
            .parse::<u64>()
            .map_err(|_| "ValidateSamFile invalid alignment position".to_string())?;
        for issue in structure.observe(
            flags,
            fields[2],
            position,
            fields[5],
            if fields[9] == "*" { 0 } else { fields[9].len() },
            (fields[10] != "*").then_some(fields[10].len()),
        )? {
            add_validate_issue(
                &mut report,
                issue,
                format!(
                    "{}:Record {record_number}, Read name {}",
                    issue.replace(':', "::"),
                    fields[0]
                ),
            );
        }
        if !skip_mate_validation && flags & 0x1 != 0 {
            validate_sam_mate_summary_sam_line(
                &line,
                &structure.targets,
                &mut pending_mates,
                &mut report,
            )?;
        }
    }

    if target_count == 0 {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "ERROR::MISSING_SEQUENCE_DICTIONARY:Sequence dictionary is empty".to_string(),
        );
    }
    if read_groups.is_empty() {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_READ_GROUP",
            "ERROR::MISSING_READ_GROUP:Read groups is empty".to_string(),
        );
    } else {
        for has_platform in read_groups.values() {
            if !has_platform {
                add_validate_issue(
                    &mut report,
                    "ERROR:MISSING_PLATFORM_VALUE",
                    "ERROR::MISSING_PLATFORM_VALUE:A platform value is missing".to_string(),
                );
            }
        }
    }
    for _ in pending_mates.values() {
        add_validate_issue(
            &mut report,
            "ERROR:MATE_NOT_FOUND",
            "ERROR::MATE_NOT_FOUND:Mate not found for paired read".to_string(),
        );
    }

    Ok(report)
}

fn validate_sam_record_summary_sam_line(
    line: &[u8],
    record_number: u64,
    target_count: u32,
    read_groups: &BTreeMap<String, bool>,
    report: &mut ValidateSamReport,
) -> Result<u16, String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    let qname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let read_name = String::from_utf8_lossy(qname).into_owned();
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let rname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let _pos = fields.next();
    let mapq = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    for _ in 0..5 {
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    }
    let mut read_group = None::<&[u8]>;
    let mut has_nm = false;
    for field in fields {
        if let Some(value) = field.strip_prefix(b"RG:Z:") {
            read_group = Some(value);
        }
        if field.starts_with(b"NM:") {
            has_nm = true;
        }
    }
    match read_group {
        Some(read_group) => {
            let read_group = String::from_utf8_lossy(read_group);
            if !read_groups.contains_key(read_group.as_ref()) {
                add_validate_issue(
                    report,
                    "ERROR:READ_GROUP_NOT_FOUND",
                    format!(
                        "ERROR::READ_GROUP_NOT_FOUND:Read name {read_name}, RG ID on record not found in header"
                    ),
                );
            }
        }
        None => add_validate_issue(
            report,
            "WARNING:RECORD_MISSING_READ_GROUP",
            format!(
                "WARNING::RECORD_MISSING_READ_GROUP:Read name {read_name}, A record is missing a read group"
            ),
        ),
    }

    if rname != b"*" {
        if target_count == 0 {
            add_validate_issue(
                report,
                "ERROR:MISSING_SEQUENCE_DICTIONARY",
                format!(
                    "ERROR::MISSING_SEQUENCE_DICTIONARY:Read name {read_name}, Reference sequence is missing from the sequence dictionary"
                ),
            );
        }
        if !has_nm {
            add_validate_issue(
                report,
                "WARNING:MISSING_TAG_NM",
                format!(
                    "WARNING::MISSING_TAG_NM:Record {record_number}, Read name {read_name}, NM tag (nucleotide differences) is missing"
                ),
            );
        }
    } else if mapq != b"0" {
        add_validate_issue(
            report,
            "ERROR:INVALID_MAPPING_QUALITY",
            format!(
                "ERROR::INVALID_MAPPING_QUALITY:Record {record_number}, Read name {read_name}, MAPQ should be 0 for unmapped read"
            ),
        );
    }
    Ok(flags)
}

fn validate_sam_mate_summary_sam_line(
    line: &[u8],
    targets: &BTreeMap<String, (usize, u64)>,
    pending_mates: &mut BTreeMap<Vec<u8>, ValidateSamMate>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    let qname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?
        .to_vec();
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 || flags & 0x800 != 0 {
        return Ok(());
    }
    if flags & 0x1 == 0 {
        return Ok(());
    }
    let rname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let pos = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let pos = pos.saturating_sub(1);
    for _ in 0..2 {
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    }
    let rnext = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let pnext = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let pnext = pnext.saturating_sub(1);
    let target_id = |name: &[u8]| -> i32 {
        std::str::from_utf8(name)
            .ok()
            .and_then(|name| targets.get(name))
            .map_or(-1, |(index, _)| i32_or_max(*index))
    };
    let tid = target_id(rname);
    let mtid = if rnext == b"=" { tid } else { target_id(rnext) };
    let mate = ValidateSamMate {
        tid,
        pos,
        mtid,
        mpos: pnext,
    };
    if let Some(pending) = pending_mates.remove(&qname) {
        if !pending.is_reciprocal_with(&mate) {
            add_validate_issue(
                report,
                "ERROR:MATE_NOT_FOUND",
                format!(
                    "ERROR::MATE_NOT_FOUND:Read name {}, Mate not found for paired read",
                    String::from_utf8_lossy(&qname)
                ),
            );
        }
    } else {
        pending_mates.insert(qname, mate);
    }
    Ok(())
}

#[derive(Clone)]
struct ValidateSamMate {
    tid: i32,
    pos: i64,
    mtid: i32,
    mpos: i64,
}

impl ValidateSamMate {
    fn from_record(record: &bam::Record) -> Self {
        Self {
            tid: record.tid(),
            pos: record.pos(),
            mtid: record.mtid(),
            mpos: record.mpos(),
        }
    }

    fn is_reciprocal_with(&self, mate: &Self) -> bool {
        self.mtid == mate.tid
            && self.mpos == mate.pos
            && mate.mtid == self.tid
            && mate.mpos == self.pos
    }
}

fn validate_sam_mate_summary(
    record: &bam::Record,
    pending_mates: &mut BTreeMap<Vec<u8>, ValidateSamMate>,
    report: &mut ValidateSamReport,
) {
    if record.is_secondary() || record.is_supplementary() {
        return;
    }
    let qname = record.qname().to_vec();
    let mate = ValidateSamMate::from_record(record);
    if let Some(pending) = pending_mates.remove(&qname) {
        if !pending.is_reciprocal_with(&mate) {
            add_validate_issue(
                report,
                "ERROR:MATE_NOT_FOUND",
                format!(
                    "ERROR::MATE_NOT_FOUND:Read name {}, Mate not found for paired read",
                    validate_qname(record)
                ),
            );
        }
    } else {
        pending_mates.insert(qname, mate);
    }
}

fn validate_sam_record_summary(
    record: &bam::Record,
    record_number: u64,
    target_count: u32,
    read_groups: &BTreeMap<String, bool>,
    references_by_tid: Option<&[Option<&[u8]>]>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    let read_name = validate_qname(record);
    match record.aux(b"RG") {
        Ok(Aux::String(read_group)) => {
            if !read_groups.contains_key(read_group) {
                add_validate_issue(
                    report,
                    "ERROR:READ_GROUP_NOT_FOUND",
                    format!(
                        "ERROR::READ_GROUP_NOT_FOUND:Read name {read_name}, RG ID on record not found in header"
                    ),
                );
            }
        }
        Ok(_) => add_validate_issue(
            report,
            "ERROR:INVALID_TAG_TYPE",
            format!("ERROR::INVALID_TAG_TYPE:Read name {read_name}, RG tag has invalid type"),
        ),
        Err(_) => add_validate_issue(
            report,
            "WARNING:RECORD_MISSING_READ_GROUP",
            format!(
                "WARNING::RECORD_MISSING_READ_GROUP:Read name {read_name}, A record is missing a read group"
            ),
        ),
    }

    if !record.is_unmapped() {
        if record.tid() < 0 || u32_or_max(record.tid()) >= target_count {
            add_validate_issue(
                report,
                "ERROR:MISSING_SEQUENCE_DICTIONARY",
                format!(
                    "ERROR::MISSING_SEQUENCE_DICTIONARY:Read name {read_name}, Reference sequence is missing from the sequence dictionary"
                ),
            );
        }
        match record.aux(b"NM") {
            Ok(aux) => {
                if let Some(references_by_tid) = references_by_tid
                    && let Some(actual_nm) = aux_i32(aux)
                    && let Some(expected_nm) = expected_record_nm(record, references_by_tid)?
                    && actual_nm != expected_nm
                {
                    add_validate_issue(
                        report,
                        "ERROR:INVALID_TAG_NM",
                        format!(
                            "ERROR::INVALID_TAG_NM:Record {record_number}, Read name {read_name}, NM tag is incorrect"
                        ),
                    );
                }
            }
            Err(_) => {
                add_validate_issue(
                    report,
                    "WARNING:MISSING_TAG_NM",
                    format!(
                        "WARNING::MISSING_TAG_NM:Record {record_number}, Read name {read_name}, NM tag (nucleotide differences) is missing"
                    ),
                );
            }
        }
    } else if record.mapq() != 0 {
        add_validate_issue(
            report,
            "ERROR:INVALID_MAPPING_QUALITY",
            format!(
                "ERROR::INVALID_MAPPING_QUALITY:Record {record_number}, Read name {read_name}, MAPQ should be 0 for unmapped read"
            ),
        );
    }
    Ok(())
}

fn expected_record_nm(
    record: &bam::Record,
    references_by_tid: &[Option<&[u8]>],
) -> Result<Option<i32>, String> {
    if record.is_unmapped() || record.is_secondary() || record.is_supplementary() {
        return Ok(None);
    }
    if record.tid() < 0 || record.pos() < 0 {
        return Ok(None);
    }
    let Some(reference) = references_by_tid
        .get(usize_or_max(record.tid()))
        .copied()
        .flatten()
    else {
        return Ok(None);
    };
    let read_bases = record.seq();
    let mut read_offset = 0usize;
    let mut ref_offset = usize_or_max(record.pos());
    let mut nm = 0i32;

    for cigar in &record.cigar() {
        match *cigar {
            Cigar::Match(length) => {
                for _ in 0..length {
                    if read_offset >= read_bases.len() {
                        return Err("ValidateSamFile read sequence shorter than CIGAR".to_string());
                    }
                    let Some(ref_base) = reference.get(ref_offset).copied() else {
                        return Ok(None);
                    };
                    if !dna_bases_equal(read_bases[read_offset], ref_base) {
                        nm += 1;
                    }
                    read_offset += 1;
                    ref_offset += 1;
                }
            }
            Cigar::Equal(length) => {
                read_offset += length as usize;
                ref_offset += length as usize;
            }
            Cigar::Diff(length) => {
                read_offset += length as usize;
                ref_offset += length as usize;
                nm += i32_or_max(length);
            }
            Cigar::Ins(length) => {
                read_offset += length as usize;
                nm += i32_or_max(length);
            }
            Cigar::Del(length) => {
                ref_offset += length as usize;
                nm += i32_or_max(length);
            }
            Cigar::SoftClip(length) => {
                read_offset += length as usize;
            }
            Cigar::HardClip(_) | Cigar::Pad(_) => {}
            Cigar::RefSkip(length) => {
                ref_offset += length as usize;
            }
        }
    }

    Ok(Some(nm))
}

fn validate_sam_ignored_summary_keys(args: &PicardArgs) -> Result<BTreeSet<String>, CommandError> {
    let mut ignored = BTreeSet::new();
    for value in args.get("IGNORE").into_iter().flatten() {
        let key = match value.as_str() {
            "MISSING_SEQUENCE_DICTIONARY" => "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "MISSING_READ_GROUP" => "ERROR:MISSING_READ_GROUP",
            "MISSING_PLATFORM_VALUE" => "ERROR:MISSING_PLATFORM_VALUE",
            "MISSING_TAG_NM" => "WARNING:MISSING_TAG_NM",
            "INVALID_TAG_NM" => "ERROR:INVALID_TAG_NM",
            "READ_GROUP_NOT_FOUND" => "ERROR:READ_GROUP_NOT_FOUND",
            "INVALID_TAG_TYPE" => "ERROR:INVALID_TAG_TYPE",
            "INVALID_MAPPING_QUALITY" => "ERROR:INVALID_MAPPING_QUALITY",
            "RECORD_MISSING_READ_GROUP" => "WARNING:RECORD_MISSING_READ_GROUP",
            "MATE_NOT_FOUND" => "ERROR:MATE_NOT_FOUND",
            "MISMATCH_SEQ_QUAL_LENGTH" => "ERROR:MISMATCH_SEQ_QUAL_LENGTH",
            "MISMATCH_CIGAR_SEQ_LENGTH" => "ERROR:MISMATCH_CIGAR_SEQ_LENGTH",
            "CIGAR_MAPS_OFF_REFERENCE" => "ERROR:CIGAR_MAPS_OFF_REFERENCE",
            "RECORD_OUT_OF_ORDER" => "ERROR:RECORD_OUT_OF_ORDER",
            "INVALID_ALIGNMENT_START" => "ERROR:INVALID_ALIGNMENT_START",
            _ => {
                return Err(unsupported(format!(
                    "unsupported ValidateSamFile IGNORE={value}"
                )));
            }
        };
        ignored.insert(key.to_string());
    }
    Ok(ignored)
}

fn validate_sam_mode(args: &PicardArgs) -> Result<ValidateSamMode, CommandError> {
    match optional_scalar(args, "MODE")?
        .unwrap_or_else(|| "SUMMARY".to_string())
        .to_ascii_uppercase()
        .as_str()
    {
        "SUMMARY" => Ok(ValidateSamMode::Summary),
        "VERBOSE" => Ok(ValidateSamMode::Verbose),
        mode => Err(unsupported(format!(
            "unsupported ValidateSamFile MODE={mode}"
        ))),
    }
}

fn read_group_platforms(header_text: &str) -> BTreeMap<String, bool> {
    let mut read_groups = BTreeMap::new();
    for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
        if let Some(id) = read_group_id(line) {
            let has_platform = line.split('\t').skip(1).any(|field| {
                field
                    .strip_prefix("PL:")
                    .is_some_and(|value| !value.is_empty())
            });
            read_groups.insert(id, has_platform);
        }
    }
    read_groups
}

fn validate_qname(record: &bam::Record) -> String {
    String::from_utf8_lossy(record.qname()).into_owned()
}

fn add_validate_issue(report: &mut ValidateSamReport, key: &str, detail: String) {
    add_validate_count(report, key);
    report.details.push(ValidateSamDetail {
        key: key.to_string(),
        line: detail,
    });
}

fn add_validate_count(report: &mut ValidateSamReport, key: &str) {
    *report.counts.entry(key.to_string()).or_default() += 1;
}

fn write_validate_sam_summary(
    output: Option<&str>,
    counts: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let text = if counts.is_empty() {
        "No errors found\n".to_string()
    } else {
        let mut text = "\n\n## HISTOGRAM\tjava.lang.String\nError Type\tCount\n".to_string();
        for (key, count) in counts {
            let _ = writeln!(text, "{key}\t{count}");
        }
        text.push('\n');
        text
    };
    match output {
        Some(output) => fs::write(output, text).map_err(|error| error.to_string()),
        None => std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string()),
    }
}

fn write_validate_sam_verbose(
    output: Option<&str>,
    details: &[ValidateSamDetail],
    max_output: Option<u32>,
) -> Result<(), String> {
    let text = if details.is_empty() {
        "No errors found\n".to_string()
    } else {
        let mut text = String::new();
        let max_output = max_output.map_or(100, |value| value as usize);
        for detail in details.iter().take(max_output) {
            text.push_str(&detail.line);
            text.push('\n');
        }
        if details.len() > max_output {
            let _ = writeln!(text, "Maximum output of [{max_output}] errors reached.");
        }
        text
    };
    match output {
        Some(output) => fs::write(output, text).map_err(|error| error.to_string()),
        None => std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string()),
    }
}

fn open_bam_reader_for_validate_sam_file(
    path: impl AsRef<Path>,
    reference: Option<&str>,
) -> Result<bam::Reader, CommandError> {
    let path = path.as_ref();
    let mut reader = open_bam_reader_with_reference(path, reference)?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cram"))
    {
        reader
            .set_cram_options(
                hts_sys::hts_fmt_option_CRAM_OPT_DECODE_MD,
                0 as hts_sys::sam_fields,
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(reader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_htslib::bam::record::CigarString;

    #[test]
    fn expected_nm_honors_equal_and_diff_cigar_operators() {
        let mut equal_record = bam::Record::new();
        equal_record.set(
            b"eq",
            Some(&CigarString(vec![Cigar::Equal(4)])),
            b"TTTT",
            b"FFFF",
        );
        equal_record.set_tid(0);
        equal_record.set_pos(0);
        equal_record.set_flags(0);

        let mut diff_record = bam::Record::new();
        diff_record.set(
            b"diff",
            Some(&CigarString(vec![Cigar::Diff(4)])),
            b"AAAA",
            b"FFFF",
        );
        diff_record.set_tid(0);
        diff_record.set_pos(0);
        diff_record.set_flags(0);

        let reference = [Some(&b"AAAA"[..])];
        assert_eq!(
            expected_record_nm(&equal_record, &reference).expect("NM computes"),
            Some(0)
        );
        assert_eq!(
            expected_record_nm(&diff_record, &reference).expect("NM computes"),
            Some(4)
        );
    }

    #[test]
    fn validatesam_sam_text_reports_empty_platform_value() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let path = dir.path().join("empty-pl.sam");
        fs::write(
            &path,
            "@HD\tVN:1.6\tSO:unknown\n@SQ\tSN:chr1\tLN:10\n@RG\tID:rg1\tSM:s1\tPL:\n",
        )
        .expect("fixture is written");

        let report = validate_sam_summary_sam_text(path.to_str().unwrap(), true)
            .expect("validation completes");

        assert_eq!(
            report.counts.get("ERROR:MISSING_PLATFORM_VALUE").copied(),
            Some(1)
        );
    }

    #[test]
    fn validation_issue_exit_codes_match_picard_matrix() {
        let empty = BTreeMap::new();
        assert_eq!(validatesamfile_validation_issue_exit_code(&empty), 0);
        let warn_only = BTreeMap::from([("WARNING:MISSING_TAG_NM".to_string(), 1)]);
        assert_eq!(validatesamfile_validation_issue_exit_code(&warn_only), 1);
        let both = BTreeMap::from([
            ("ERROR:INVALID_CIGAR".to_string(), 1),
            ("WARNING:MISSING_TAG_NM".to_string(), 1),
        ]);
        assert_eq!(validatesamfile_validation_issue_exit_code(&both), 2);
        let err_only = BTreeMap::from([("ERROR:INVALID_CIGAR".to_string(), 1)]);
        assert_eq!(validatesamfile_validation_issue_exit_code(&err_only), 3);
    }
}
