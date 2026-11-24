use crate::command_error::{CommandError, unsupported};
use crate::{
    dictionary_contig_order, has_extension, has_gzip_extension, open_text_or_gzip_reader,
    optional_bool, optional_scalar, optional_u32, read_text_or_gzip, reference_sequences_by_name,
    reject_unsupported_compression_level, required_scalar_for, required_values_for, usize_or_max,
    vcf_headers, write_text_or_gzip,
};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter};
use turbo_picard_core::picard_args::{PicardArgs, normalize_picard_args_for_command};

pub(crate) fn run_updatevcfsequencedictionary(args: &[String]) -> Result<(), CommandError> {
    let args = normalize_picard_args_for_command("UpdateVcfSequenceDictionary", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_updatevcfsequencedictionary_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "UpdateVcfSequenceDictionary")?;
    let output = required_scalar_for(&args, "OUTPUT", "UpdateVcfSequenceDictionary")?;
    let dictionary_path =
        required_scalar_for(&args, "SEQUENCE_DICTIONARY", "UpdateVcfSequenceDictionary")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let dictionary_text = fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
    let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
    let mut input_reader = open_text_or_gzip_reader(&input)?;
    let mut output_writer = VcfOutput::create(&output, create_index)?;
    let mut line = String::new();
    let mut inserted_contigs = false;
    let mut saw_column_header = false;

    loop {
        line.clear();
        if input_reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with("##contig=<") {
            continue;
        }
        if line.starts_with("#CHROM") && !inserted_contigs {
            for contig in &contig_lines {
                output_writer.write_header(contig)?;
                output_writer.write_header("\n")?;
            }
            inserted_contigs = true;
        }
        if line.starts_with("#CHROM") {
            saw_column_header = true;
        }
        if line.starts_with('#') {
            output_writer.write_header(line)?;
            output_writer.write_header("\n")?;
        } else if !line.trim().is_empty() {
            output_writer.write_record(line.as_bytes())?;
        }
    }

    if !saw_column_header {
        return Err("VCF input is missing #CHROM header".into());
    }
    Ok(output_writer.finish()?)
}

pub(crate) fn run_liftovervcf(args: &[String]) -> Result<(), CommandError> {
    let args = normalize_picard_args_for_command("LiftoverVcf", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_liftovervcf_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "LiftoverVcf")?;
    let output = required_scalar_for(&args, "OUTPUT", "LiftoverVcf")?;
    let chain = required_scalar_for(&args, "CHAIN", "LiftoverVcf")?;
    let reject = required_scalar_for(&args, "REJECT", "LiftoverVcf")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "LiftoverVcf")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let mappings = read_simple_chain_mappings(&chain)?;
    let document = read_vcf_document(&input)?;
    let reference_sequences = reference_sequences_by_name(&reference)?;
    let dictionary_text = fs::read_to_string(reference_dictionary_path(&reference)?)
        .map_err(|error| error.to_string())?;
    let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
    let contig_order = dictionary_contig_order(&dictionary_text);
    let reference_line = format!("##reference=file:{reference}");

    if let Some((output_text, reject_text)) = liftover_identity_vcf_text(
        &document,
        &mappings,
        &reference_sequences,
        &contig_order,
        &contig_lines,
        &reference_line,
    )? {
        write_text_or_gzip(&output, &output_text)?;
        if create_index && has_extension(&output, "vcf") {
            write_vcf_idx_sidecar(&output, &output_text)?;
        }
        return Ok(write_text_or_gzip(&reject, &reject_text)?);
    }

    let mut lifted = Vec::with_capacity(document.records.len());
    let mut rejected = Vec::new();
    for record in document.records.iter().cloned() {
        match liftover_vcf_record(record, &mappings, &reference_sequences)? {
            LiftoverRecordResult::Lifted(record) => lifted.push(record),
            LiftoverRecordResult::Rejected(record) => rejected.push(record),
        }
    }

    for record in &lifted {
        if !contig_order.contains_key(&record.contig) {
            return Err(format!(
                "VCF contig {} is not present in sequence dictionary",
                record.contig
            )
            .into());
        }
    }
    lifted.sort_by(|left, right| {
        contig_order[&left.contig]
            .cmp(&contig_order[&right.contig])
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.serial.cmp(&right.serial))
    });

    let output_text = liftover_output_vcf_text(&document, &contig_lines, &reference_line, &lifted);
    let reject_text = liftover_reject_vcf_text(&document, &contig_lines, &rejected);
    write_text_or_gzip(&output, &output_text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &output_text)?;
    }
    Ok(write_text_or_gzip(&reject, &reject_text)?)
}

pub(crate) fn run_gathervcfs(args: &[String]) -> Result<(), CommandError> {
    let args =
        normalize_picard_args_for_command("GatherVcfs", args).map_err(|error| error.to_string())?;
    reject_unsupported_gathervcfs_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "GatherVcfs")?;
    let output = required_scalar_for(&args, "OUTPUT", "GatherVcfs")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let headers = inputs
        .iter()
        .map(|input| read_vcf_header(input))
        .collect::<Result<Vec<_>, _>>()?;
    let first = headers
        .first()
        .ok_or_else(|| "missing required GatherVcfs argument: INPUT".to_string())?;
    let first_contig_ids = first.contig_ids();
    for header in headers.iter().skip(1) {
        if header.column_header != first.column_header {
            return Err(unsupported(
                "unsupported GatherVcfs inputs with different sample columns",
            ));
        }
        if header.contig_ids() != first_contig_ids {
            return Err(unsupported(
                "unsupported GatherVcfs inputs with different sequence dictionaries",
            ));
        }
    }
    vcf_headers::validate_gather(&inputs, first)?;
    let mut output_writer = VcfOutput::create(&output, create_index)?;
    output_writer.write_header(&first.header_text())?;
    for input in &inputs {
        stream_vcf_records(input, |record| {
            Ok(output_writer.write_record(record.line.as_bytes())?)
        })?;
    }
    Ok(output_writer.finish()?)
}

pub(crate) fn run_sortvcf(args: &[String]) -> Result<(), CommandError> {
    run_sorted_vcf_merge(
        "SortVcf",
        "turbo-picard-sortvcf",
        reject_unsupported_sortvcf_args,
        args,
    )
}

pub(crate) fn run_mergevcfs(args: &[String]) -> Result<(), CommandError> {
    run_sorted_vcf_merge(
        "MergeVcfs",
        "turbo-picard-mergevcfs",
        reject_unsupported_mergevcfs_args,
        args,
    )
}

fn run_sorted_vcf_merge(
    command_name: &'static str,
    sorter_prefix: &str,
    reject_args: fn(&PicardArgs) -> Result<(), CommandError>,
    args: &[String],
) -> Result<(), CommandError> {
    let args =
        normalize_picard_args_for_command(command_name, args).map_err(|error| error.to_string())?;
    reject_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", command_name)?;
    let output = required_scalar_for(&args, "OUTPUT", command_name)?;
    let dictionary_path = optional_scalar(&args, "SEQUENCE_DICTIONARY")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let max_records_in_ram =
        optional_u32(&args, "MAX_RECORDS_IN_RAM")?.map_or(500_000, |value| value as usize);
    let tmp_dir = optional_scalar(&args, "TMP_DIR")?
        .filter(|value| !value.trim().is_empty())
        .map_or_else(env::temp_dir, std::path::PathBuf::from);

    let documents = inputs
        .iter()
        .map(|input| read_vcf_header(input))
        .collect::<Result<Vec<_>, _>>()?;
    let first = documents
        .first()
        .ok_or_else(|| format!("missing required {command_name} argument: INPUT"))?;
    for document in documents.iter().skip(1) {
        if document.column_header != first.column_header {
            return Err(unsupported(format!(
                "unsupported {command_name} inputs with different sample columns"
            )));
        }
    }

    let (contig_order, contig_lines) = if let Some(dictionary_path) = dictionary_path {
        let dictionary_text =
            fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
        let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, command_name)?;
        (
            dictionary_contig_order(&dictionary_text),
            Some(contig_lines),
        )
    } else {
        let contig_lines = first.contig_lines();
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, command_name)?;
        (first.contig_order(), None)
    };
    if contig_order.is_empty() {
        return Err(unsupported(format!(
            "unsupported {command_name} input without sequence dictionary"
        )));
    }

    let mut config = ExternalSortConfig::new(&tmp_dir);
    config.prefix = sorter_prefix.to_string();
    config.max_records_in_ram = max_records_in_ram.max(1);
    let mut sorter = ExternalSorter::new(config)?;
    push_vcf_records_into_sorter(&inputs, &contig_order, &mut sorter)?;

    let merged_header = vcf_headers::merge(&documents)?;
    let mut output_writer = VcfOutput::create(&output, create_index)?;
    output_writer.write_header(&vcf_header_text_with_contigs(
        &merged_header,
        contig_lines.as_deref(),
    )?)?;
    sorter.finish_into(|record| {
        output_writer
            .write_record(&record.payload)
            .map_err(Into::into)
    })?;
    Ok(output_writer.finish()?)
}

fn reject_unsupported_updatevcfsequencedictionary_args(
    args: &PicardArgs,
) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "COMPRESSION_LEVEL",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported UpdateVcfSequenceDictionary argument: {key}"
            )));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    reject_unsupported_compression_level(args, "UpdateVcfSequenceDictionary")?;
    Ok(())
}

fn reject_unsupported_liftovervcf_args(args: &PicardArgs) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "CHAIN",
        "REJECT",
        "REFERENCE_SEQUENCE",
        "WARN_ON_MISSING_CONTIG",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported LiftoverVcf argument: {key}"
            )));
        }
    }
    optional_bool(args, "WARN_ON_MISSING_CONTIG")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    reject_unsupported_compression_level(args, "LiftoverVcf")?;
    Ok(())
}

fn reject_unsupported_gathervcfs_args(args: &PicardArgs) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported GatherVcfs argument: {key}"
            )));
        }
    }
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    reject_unsupported_compression_level(args, "GatherVcfs")?;
    Ok(())
}

fn reject_unsupported_sortvcf_args(args: &PicardArgs) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "COMPRESSION_LEVEL",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!("unsupported SortVcf argument: {key}")));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    reject_unsupported_compression_level(args, "SortVcf")?;
    Ok(())
}

fn reject_unsupported_mergevcfs_args(args: &PicardArgs) -> Result<(), CommandError> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported MergeVcfs argument: {key}"
            )));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    reject_unsupported_compression_level(args, "MergeVcfs")?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct VcfDocument {
    pub(crate) meta_lines: Vec<String>,
    pub(crate) column_header: String,
    pub(crate) records: Vec<VcfRecord>,
}

impl VcfDocument {
    fn header_text(&self) -> String {
        let mut text = String::new();
        for line in &self.meta_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str(&self.column_header);
        text.push('\n');
        text
    }

    pub(crate) fn contig_ids(&self) -> Vec<String> {
        self.contig_lines()
            .iter()
            .filter_map(|line| parse_vcf_contig_id(line.as_str()))
            .collect()
    }

    fn contig_lines(&self) -> Vec<String> {
        self.meta_lines
            .iter()
            .filter(|line| line.starts_with("##contig=<"))
            .cloned()
            .collect()
    }

    pub(crate) fn contig_order(&self) -> BTreeMap<String, usize> {
        self.contig_ids()
            .into_iter()
            .enumerate()
            .map(|(index, contig)| (contig, index))
            .collect()
    }
}

fn validate_vcf_sequence_dictionaries(
    documents: &[VcfDocument],
    expected_contig_lines: &[String],
    command: &str,
) -> Result<(), CommandError> {
    for document in documents {
        if document.contig_lines() != expected_contig_lines {
            return Err(unsupported(format!(
                "unsupported {command} input sequence dictionary differs from expected dictionary"
            )));
        }
    }
    Ok(())
}

fn vcf_header_text_with_contigs(
    document: &VcfDocument,
    contig_lines: Option<&[String]>,
) -> Result<String, String> {
    let header_text = document.header_text();
    if let Some(contig_lines) = contig_lines {
        replace_vcf_contig_header(&header_text, contig_lines)
    } else {
        Ok(header_text)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VcfRecord {
    pub(crate) line: String,
    pub(crate) contig: String,
    pub(crate) position: u64,
    serial: usize,
}

fn read_vcf_document(path: &str) -> Result<VcfDocument, CommandError> {
    let text = read_text_or_gzip(path)?;
    parse_vcf_document(&text, path)
}

fn read_vcf_header(path: &str) -> Result<VcfDocument, CommandError> {
    let mut reader = open_text_or_gzip_reader(path)?;
    let mut meta_lines = Vec::new();
    let mut column_header = None;
    let mut line = String::new();
    let mut line_number = 0usize;

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        line_number += 1;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with("##") {
            if column_header.is_some() {
                return Err(format!("malformed VCF header in {path}").into());
            }
            meta_lines.push(line.to_string());
        } else if line.starts_with("#CHROM") {
            column_header = Some(line.to_string());
        } else if line.starts_with('#') {
            return Err(unsupported(format!(
                "unsupported VCF header line {line_number} in {path}"
            )));
        } else if !line.trim().is_empty() {
            if column_header.is_none() {
                return Err(format!("VCF input {path} is missing #CHROM header").into());
            }
            parse_vcf_record(line, 0, path, line_number)?;
            break;
        }
    }

    let column_header =
        column_header.ok_or_else(|| format!("VCF input {path} is missing #CHROM header"))?;
    Ok(VcfDocument {
        meta_lines,
        column_header,
        records: Vec::new(),
    })
}

pub(crate) fn stream_vcf_records(
    path: &str,
    mut visit: impl FnMut(VcfRecord) -> Result<(), CommandError>,
) -> Result<(), CommandError> {
    let mut reader = open_text_or_gzip_reader(path)?;
    let mut column_header = false;
    let mut line = String::new();
    let mut line_number = 0usize;
    let mut serial = 0usize;

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        line_number += 1;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with("##") {
            if column_header {
                return Err(format!("malformed VCF header in {path}").into());
            }
        } else if line.starts_with("#CHROM") {
            column_header = true;
        } else if line.starts_with('#') {
            return Err(unsupported(format!(
                "unsupported VCF header line {line_number} in {path}"
            )));
        } else if !line.trim().is_empty() {
            if !column_header {
                return Err(format!("VCF input {path} is missing #CHROM header").into());
            }
            let record = parse_vcf_record(line, serial, path, line_number)?;
            serial = serial
                .checked_add(1)
                .ok_or_else(|| "VCF input record count overflow".to_string())?;
            visit(record)?;
        }
    }
    if !column_header {
        return Err(format!("VCF input {path} is missing #CHROM header").into());
    }
    Ok(())
}

fn push_vcf_records_into_sorter(
    inputs: &[String],
    contig_order: &BTreeMap<String, usize>,
    sorter: &mut ExternalSorter,
) -> Result<(), CommandError> {
    let mut serial = 0u64;
    for input in inputs {
        stream_vcf_records(input, |record| {
            let contig_index = contig_order.get(&record.contig).copied().ok_or_else(|| {
                format!(
                    "VCF contig {} is not present in sequence dictionary",
                    record.contig
                )
            })?;
            sorter.push(
                vcf_sort_key(contig_index, record.position, serial),
                record.line.into_bytes(),
            )?;
            serial = serial
                .checked_add(1)
                .ok_or_else(|| "VCF input record count overflow".to_string())?;
            Ok(())
        })?;
    }
    Ok(())
}

fn vcf_sort_key(contig_index: usize, position: u64, serial: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(24);
    key.extend_from_slice(&(contig_index as u64).to_be_bytes());
    key.extend_from_slice(&position.to_be_bytes());
    key.extend_from_slice(&serial.to_be_bytes());
    key
}

fn parse_vcf_document(text: &str, source: &str) -> Result<VcfDocument, CommandError> {
    let mut meta_lines = Vec::new();
    let mut column_header = None;
    let mut records = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        if line.starts_with("##") {
            if column_header.is_some() {
                return Err(format!("malformed VCF header in {source}").into());
            }
            meta_lines.push(line.to_string());
        } else if line.starts_with("#CHROM") {
            column_header = Some(line.to_string());
        } else if line.starts_with('#') {
            return Err(unsupported(format!(
                "unsupported VCF header line {} in {source}",
                line_index + 1
            )));
        } else if !line.trim().is_empty() {
            if column_header.is_none() {
                return Err(format!("VCF input {source} is missing #CHROM header").into());
            }
            records.push(parse_vcf_record(
                line,
                records.len(),
                source,
                line_index + 1,
            )?);
        }
    }

    let column_header =
        column_header.ok_or_else(|| format!("VCF input {source} is missing #CHROM header"))?;
    Ok(VcfDocument {
        meta_lines,
        column_header,
        records,
    })
}

fn parse_vcf_record(
    line: &str,
    serial: usize,
    source: &str,
    line_number: usize,
) -> Result<VcfRecord, String> {
    let mut fields = line.split('\t');
    let contig = fields
        .next()
        .ok_or_else(|| format!("malformed VCF record on line {line_number} in {source}"))?;
    let position = fields
        .next()
        .ok_or_else(|| format!("malformed VCF record on line {line_number} in {source}"))?
        .parse::<u64>()
        .map_err(|_| format!("malformed VCF POS on line {line_number} in {source}"))?;
    Ok(VcfRecord {
        line: line.to_string(),
        contig: contig.to_string(),
        position,
        serial,
    })
}

#[derive(Debug, Clone)]
struct ChainMapping {
    source_contig: String,
    source_start: u64,
    source_end: u64,
    target_contig: String,
    target_start: u64,
    target_end: u64,
}

enum LiftoverRecordResult {
    Lifted(VcfRecord),
    Rejected(VcfRecord),
}

fn read_simple_chain_mappings(path: &str) -> Result<Vec<ChainMapping>, CommandError> {
    let text = read_text_or_gzip(path)?;
    let mut mappings = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(header) = line.strip_prefix("chain ") else {
            return Err(unsupported(
                "unsupported LiftoverVcf chain file without chain header",
            ));
        };
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 12 {
            return Err(unsupported(
                "unsupported LiftoverVcf malformed chain header",
            ));
        }
        let target_contig = fields[1].to_string();
        let target_size = fields[2]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed target size"))?;
        let target_strand = fields[3];
        let target_start = fields[4]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed target start"))?;
        let target_end = fields[5]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed target end"))?;
        let source_contig = fields[6].to_string();
        let source_size = fields[7]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed source size"))?;
        let source_strand = fields[8];
        let source_start = fields[9]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed source start"))?;
        let source_end = fields[10]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed source end"))?;
        if target_strand != "+" || source_strand != "+" {
            return Err(unsupported("unsupported LiftoverVcf reverse-strand chain"));
        }
        if target_end > target_size || source_end > source_size || target_start >= target_end {
            return Err(unsupported(
                "unsupported LiftoverVcf malformed chain bounds",
            ));
        }
        let block = lines
            .next()
            .ok_or_else(|| unsupported("unsupported LiftoverVcf chain without block"))?;
        let block_fields = block.split_whitespace().collect::<Vec<_>>();
        if block_fields.len() != 1 {
            return Err(unsupported(
                "unsupported LiftoverVcf gapped or multi-block chain",
            ));
        }
        let block_size = block_fields[0]
            .parse::<u64>()
            .map_err(|_| unsupported("unsupported LiftoverVcf malformed chain block"))?;
        if block_size != source_end - source_start || block_size != target_end - target_start {
            return Err(unsupported(
                "unsupported LiftoverVcf chain block does not span interval",
            ));
        }
        mappings.push(ChainMapping {
            source_contig,
            source_start,
            source_end,
            target_contig,
            target_start,
            target_end,
        });
    }
    if mappings.is_empty() {
        return Err(unsupported("unsupported LiftoverVcf empty chain"));
    }
    Ok(mappings)
}

fn liftover_identity_vcf_text(
    document: &VcfDocument,
    mappings: &[ChainMapping],
    reference: &BTreeMap<String, Vec<u8>>,
    contig_order: &BTreeMap<String, usize>,
    contig_lines: &[String],
    reference_line: &str,
) -> Result<Option<(String, String)>, String> {
    let mut identity_mappings = BTreeMap::new();
    for mapping in mappings {
        if mapping.source_contig != mapping.target_contig
            || mapping.source_start != mapping.target_start
            || mapping.source_end != mapping.target_end
        {
            return Ok(None);
        }
        if identity_mappings
            .insert(
                mapping.source_contig.as_str(),
                (mapping.source_start, mapping.source_end),
            )
            .is_some()
        {
            return Ok(None);
        }
    }

    let mut last_order: Option<(usize, u64)> = None;
    for record in &document.records {
        let Some(contig_rank) = contig_order.get(&record.contig) else {
            return Ok(None);
        };
        if let Some((last_contig_rank, last_position)) = last_order
            && (*contig_rank < last_contig_rank
                || (*contig_rank == last_contig_rank && record.position < last_position))
        {
            return Ok(None);
        }
        last_order = Some((*contig_rank, record.position));

        let Some((mapping_start, mapping_end)) = identity_mappings.get(record.contig.as_str())
        else {
            return Ok(None);
        };
        let ref_allele = vcf_ref_field_for_liftover(&record.line)?;
        if ref_allele == "."
            || ref_allele
                .chars()
                .any(|base| matches!(base, '<' | '>' | '[' | ']'))
        {
            return Ok(None);
        }
        let Some(source_start) = record.position.checked_sub(1) else {
            return Ok(None);
        };
        let source_end = source_start + ref_allele.len() as u64;
        if source_start < *mapping_start || source_end > *mapping_end {
            return Ok(None);
        }
        let Some(target_reference) = reference.get(&record.contig) else {
            return Err(format!(
                "LiftoverVcf reference missing contig {}",
                record.contig
            ));
        };
        let reference_slice = target_reference
            .get(usize_or_max(source_start)..usize_or_max(source_end))
            .ok_or_else(|| "LiftoverVcf target interval extends beyond reference".to_string())?;
        if !reference_slice.eq_ignore_ascii_case(ref_allele.as_bytes()) {
            return Ok(None);
        }
    }

    Ok(Some((
        liftover_output_vcf_text(document, contig_lines, reference_line, &document.records),
        liftover_reject_vcf_text(document, contig_lines, &[]),
    )))
}

fn vcf_ref_field_for_liftover(line: &str) -> Result<&str, String> {
    let mut fields = line.split('\t');
    for _ in 0..3 {
        fields
            .next()
            .ok_or_else(|| "malformed VCF record for LiftoverVcf".to_string())?;
    }
    let ref_allele = fields
        .next()
        .ok_or_else(|| "malformed VCF record for LiftoverVcf".to_string())?;
    for _ in 4..8 {
        fields
            .next()
            .ok_or_else(|| "malformed VCF record for LiftoverVcf".to_string())?;
    }
    Ok(ref_allele)
}

fn liftover_vcf_record(
    record: VcfRecord,
    mappings: &[ChainMapping],
    reference: &BTreeMap<String, Vec<u8>>,
) -> Result<LiftoverRecordResult, CommandError> {
    let fields = record.line.split('\t').collect::<Vec<_>>();
    if fields.len() < 8 {
        return Err("malformed VCF record for LiftoverVcf".into());
    }
    let ref_allele = fields[3];
    if ref_allele == "."
        || ref_allele
            .chars()
            .any(|base| matches!(base, '<' | '>' | '[' | ']'))
    {
        return Err(unsupported(
            "unsupported LiftoverVcf symbolic or missing REF allele",
        ));
    }
    let source_start = record.position - 1;
    let source_end = source_start + ref_allele.len() as u64;
    let Some(mapping) = mappings.iter().find(|mapping| {
        mapping.source_contig == record.contig
            && source_start >= mapping.source_start
            && source_end <= mapping.source_end
    }) else {
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record, "NoTarget", ".",
        )));
    };
    let target_start = mapping.target_start + (source_start - mapping.source_start);
    let target_end = target_start + ref_allele.len() as u64;
    if target_end > mapping.target_end {
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record, "NoTarget", ".",
        )));
    }
    let Some(target_reference) = reference.get(&mapping.target_contig) else {
        return Err(format!(
            "LiftoverVcf reference missing contig {}",
            mapping.target_contig
        )
        .into());
    };
    let reference_slice = target_reference
        .get(usize_or_max(target_start)..usize_or_max(target_end))
        .ok_or_else(|| "LiftoverVcf target interval extends beyond reference".to_string())?;
    if !reference_slice.eq_ignore_ascii_case(ref_allele.as_bytes()) {
        let info = format!(
            "AttemptedAlleles={}*->{};AttemptedLocus={}:{}-{}",
            fields[3],
            fields[4],
            mapping.target_contig,
            target_start + 1,
            target_end
        );
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record,
            "MismatchedRefAllele",
            &info,
        )));
    }

    if mapping.target_contig == record.contig && target_start + 1 == record.position {
        drop(fields);
        return Ok(LiftoverRecordResult::Lifted(record));
    }

    let mut lifted_fields = fields.iter().map(ToString::to_string).collect::<Vec<_>>();
    lifted_fields[0].clone_from(&mapping.target_contig);
    lifted_fields[1] = (target_start + 1).to_string();
    Ok(LiftoverRecordResult::Lifted(VcfRecord {
        line: lifted_fields.join("\t"),
        contig: mapping.target_contig.clone(),
        position: target_start + 1,
        serial: record.serial,
    }))
}

fn reject_liftover_record(record: &VcfRecord, filter: &str, info: &str) -> VcfRecord {
    let mut fields = record
        .line
        .split('\t')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if fields.len() >= 8 {
        fields[6] = filter.to_string();
        fields[7] = info.to_string();
    }
    VcfRecord {
        line: fields.join("\t"),
        contig: record.contig.clone(),
        position: record.position,
        serial: record.serial,
    }
}

fn liftover_output_vcf_text(
    document: &VcfDocument,
    contig_lines: &[String],
    reference_line: &str,
    records: &[VcfRecord],
) -> String {
    let mut text = String::new();
    for line in &document.meta_lines {
        if line.starts_with("##contig=<")
            || line.starts_with("##reference=")
            || line.starts_with("##INFO=<ID=ReverseComplementedAlleles,")
            || line.starts_with("##INFO=<ID=SwappedAlleles,")
        {
            continue;
        }
        text.push_str(line);
        text.push('\n');
        if line.starts_with("##fileformat=") {
            text.push_str("##INFO=<ID=ReverseComplementedAlleles,Number=0,Type=Flag,Description=\"The REF and the ALT alleles have been reverse complemented in liftover since the mapping from the previous reference to the current one was on the negative strand.\">\n");
            text.push_str("##INFO=<ID=SwappedAlleles,Number=0,Type=Flag,Description=\"The REF and the ALT alleles have been swapped in liftover due to changes in the reference. It is possible that not all INFO annotations reflect this swap, and in the genotypes, only the GT, PL, and AD fields have been modified. You should check the TAGS_TO_REVERSE parameter that was used during the LiftOver to be sure.\">\n");
        }
    }
    for line in contig_lines {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(reference_line);
    text.push('\n');
    text.push_str(&document.column_header);
    text.push('\n');
    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    text
}

fn liftover_reject_vcf_text(
    document: &VcfDocument,
    contig_lines: &[String],
    records: &[VcfRecord],
) -> String {
    let mut text = String::new();
    for line in &document.meta_lines {
        if line.starts_with("##contig=<")
            || line.starts_with("##FILTER=<ID=CannotLiftOver,")
            || line.starts_with("##FILTER=<ID=IndelStraddlesMultipleIntervals,")
            || line.starts_with("##FILTER=<ID=MismatchedRefAllele,")
            || line.starts_with("##FILTER=<ID=NoTarget,")
            || line.starts_with("##INFO=<ID=AttemptedAlleles,")
            || line.starts_with("##INFO=<ID=AttemptedLocus,")
        {
            continue;
        }
        text.push_str(line);
        text.push('\n');
        if line.starts_with("##fileformat=") {
            text.push_str("##FILTER=<ID=CannotLiftOver,Description=\"Liftover of a variant that needed reverse-complementing failed for unknown reasons.\">\n");
            text.push_str("##FILTER=<ID=IndelStraddlesMultipleIntervals,Description=\"Reference allele in Indel is straddling multiple intervals in the chain, and so the results are not well defined.\">\n");
            text.push_str("##FILTER=<ID=MismatchedRefAllele,Description=\"Reference allele does not match reference genome sequence after liftover.\">\n");
            text.push_str("##FILTER=<ID=NoTarget,Description=\"Variant could not be lifted between genome builds.\">\n");
            text.push_str("##INFO=<ID=AttemptedAlleles,Number=1,Type=String,Description=\"The alleles of the variant in the TARGET prior to failing due to reference allele mismatching to the target reference.\">\n");
            text.push_str("##INFO=<ID=AttemptedLocus,Number=1,Type=String,Description=\"The locus of the variant in the TARGET prior to failing due to reference allele mismatching to the target reference.\">\n");
        }
    }
    for line in contig_lines {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&document.column_header);
    text.push('\n');
    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    text
}

fn reference_dictionary_path(reference: &str) -> Result<String, String> {
    let path = Path::new(reference);
    let stem_dict = path.with_extension("dict");
    if stem_dict.exists() {
        return Ok(stem_dict.display().to_string());
    }
    let fa_dict = format!("{reference}.dict");
    if Path::new(&fa_dict).exists() {
        return Ok(fa_dict);
    }
    Err(format!(
        "LiftoverVcf reference {reference} must have an adjacent .dict file"
    ))
}

fn parse_vcf_contig_id(line: &str) -> Option<String> {
    let body = line.strip_prefix("##contig=<")?.strip_suffix('>')?;
    body.split(',').find_map(|field| {
        field
            .strip_prefix("ID=")
            .map(|value| value.trim_matches('"').to_string())
    })
}

fn vcf_contig_lines_from_dictionary(dictionary_text: &str) -> Result<Vec<String>, String> {
    let mut contigs = Vec::new();
    for line in dictionary_text
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
    {
        let fields = line
            .split('\t')
            .skip(1)
            .filter_map(|field| field.split_once(':'))
            .collect::<BTreeMap<_, _>>();
        let id = fields
            .get("SN")
            .ok_or_else(|| "sequence dictionary @SQ line missing SN".to_string())?;
        let length = fields
            .get("LN")
            .ok_or_else(|| "sequence dictionary @SQ line missing LN".to_string())?;
        let mut attributes = vec![format!("ID={id}"), format!("length={length}")];
        if let Some(md5) = fields.get("M5") {
            attributes.push(format!("md5={md5}"));
        }
        if let Some(assembly) = fields.get("AS") {
            attributes.push(format!("assembly={assembly}"));
        }
        if let Some(species) = fields.get("SP") {
            attributes.push(format!("species={species}"));
        }
        if let Some(uri) = fields.get("UR") {
            attributes.push(format!("URI={uri}"));
        }
        contigs.push(format!("##contig=<{}>", attributes.join(",")));
    }
    if contigs.is_empty() {
        return Err("sequence dictionary contains no @SQ records".to_string());
    }
    Ok(contigs)
}

fn replace_vcf_contig_header(input_text: &str, contig_lines: &[String]) -> Result<String, String> {
    let mut output = String::new();
    let mut inserted_contigs = false;
    let mut saw_column_header = false;

    for line in input_text.lines() {
        if line.starts_with("##contig=<") {
            continue;
        }
        if line.starts_with("#CHROM") && !inserted_contigs {
            for contig in contig_lines {
                output.push_str(contig);
                output.push('\n');
            }
            inserted_contigs = true;
        }
        if line.starts_with("#CHROM") {
            saw_column_header = true;
        }
        output.push_str(line);
        output.push('\n');
    }

    if !saw_column_header {
        return Err("VCF input is missing #CHROM header".to_string());
    }
    Ok(output)
}

enum VcfOutputWriter {
    Plain(BufWriter<fs::File>),
    Gzip(GzEncoder<BufWriter<fs::File>>),
}

impl VcfOutputWriter {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Plain(writer) => writer.write_all(bytes),
            Self::Gzip(writer) => writer.write_all(bytes),
        }
        .map_err(|error| error.to_string())
    }

    fn finish(self) -> Result<(), String> {
        match self {
            Self::Plain(mut writer) => writer.flush().map_err(|error| error.to_string()),
            Self::Gzip(mut writer) => writer.try_finish().map_err(|error| error.to_string()),
        }
    }
}

struct VcfOutput {
    writer: VcfOutputWriter,
    index: Option<BufWriter<fs::File>>,
    offset: u64,
}

impl VcfOutput {
    fn create(path: &str, create_index: bool) -> Result<Self, String> {
        let file = fs::File::create(path).map_err(|error| error.to_string())?;
        let writer = if has_gzip_extension(path) {
            VcfOutputWriter::Gzip(GzEncoder::new(
                BufWriter::with_capacity(1024 * 1024, file),
                Compression::default(),
            ))
        } else {
            VcfOutputWriter::Plain(BufWriter::with_capacity(1024 * 1024, file))
        };
        let index = if create_index && has_extension(path, "vcf") {
            let index_file =
                fs::File::create(format!("{path}.idx")).map_err(|error| error.to_string())?;
            let mut index = BufWriter::with_capacity(1024 * 1024, index_file);
            index
                .write_all(b"# turbo-picard VCF record offsets\n")
                .map_err(|error| error.to_string())?;
            Some(index)
        } else {
            None
        };
        Ok(Self {
            writer,
            index,
            offset: 0,
        })
    }

    fn write_header(&mut self, text: &str) -> Result<(), String> {
        self.write_bytes(text.as_bytes())
    }

    fn write_record(&mut self, record: &[u8]) -> Result<(), String> {
        if let Some(index) = self.index.as_mut() {
            writeln!(index, "{}", self.offset).map_err(|error| error.to_string())?;
        }
        self.write_bytes(record)?;
        self.write_bytes(b"\n")
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer.write_all(bytes)?;
        self.offset = self
            .offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "VCF output offset overflow".to_string())?;
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        self.writer.finish()?;
        if let Some(mut index) = self.index.take() {
            index.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn write_vcf_idx_sidecar(output: &str, text: &str) -> Result<(), String> {
    let mut offset = 0usize;
    let mut index = String::from("# turbo-picard VCF record offsets\n");
    for line in text.split_inclusive('\n') {
        if !line.starts_with('#') {
            index.push_str(&offset.to_string());
            index.push('\n');
        }
        offset += line.len();
    }
    fs::write(format!("{output}.idx"), index).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vcf_contig_id_and_replace_header_contigs() {
        assert_eq!(
            parse_vcf_contig_id("##contig=<ID=chr1,length=248956422>"),
            Some("chr1".to_string())
        );
        assert_eq!(parse_vcf_contig_id("##INFO=<ID=DP>"), None);

        let vcf = "##fileformat=VCFv4.2
##contig=<ID=old,length=10>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	5	.	A	G	.	PASS	.
";
        let updated =
            replace_vcf_contig_header(vcf, &["##contig=<ID=chr1,length=100>".to_string()]).unwrap();
        assert!(!updated.contains("ID=old"));
        assert!(updated.contains(
            "##contig=<ID=chr1,length=100>
#CHROM"
        ));
    }
}
