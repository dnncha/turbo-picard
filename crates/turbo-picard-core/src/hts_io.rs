use crate::bgzf_threads::{HtsThreadRole, bgzf_threads_for};
use rust_htslib::{
    bam::{self, Format, Read},
    htslib,
};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HtsIoError {
    #[error("unsupported {command} output format for {output}; expected .sam, .bam, or .cram")]
    UnsupportedOutputFormat { command: String, output: String },
    #[error("CRAM path {path} requires REFERENCE_SEQUENCE (R=) or TURBO_PICARD_REFERENCE")]
    CramReferenceRequired { path: String },
    #[error(transparent)]
    Htslib(#[from] rust_htslib::errors::Error),
}

pub fn base_qualities(record: &bam::Record) -> &[u8] {
    let qualities = record.qual();
    if qualities.first() == Some(&255) {
        &[]
    } else {
        qualities
    }
}

pub fn path_extension_lower(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

pub fn path_format(path: &str) -> Option<Format> {
    match path_extension_lower(path).as_deref() {
        Some("sam") => Some(Format::Sam),
        Some("bam") => Some(Format::Bam),
        Some("cram") => Some(Format::Cram),
        _ => None,
    }
}

pub fn is_hts_container_input(path: &str) -> bool {
    matches!(path_extension_lower(path).as_deref(), Some("bam" | "cram"))
}

pub fn is_sam_text_input(path: &str) -> bool {
    path_extension_lower(path).as_deref() == Some("sam")
}

pub fn output_format_for_path(output: &str, command: &str) -> Result<Format, HtsIoError> {
    path_format(output).ok_or_else(|| HtsIoError::UnsupportedOutputFormat {
        command: command.to_string(),
        output: output.to_string(),
    })
}

pub fn resolve_reference_sequence(
    path: &str,
    explicit: Option<&str>,
) -> Result<Option<String>, HtsIoError> {
    let explicit = explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("TURBO_PICARD_REFERENCE")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });

    if path_format(path) == Some(Format::Cram) {
        explicit
            .ok_or_else(|| HtsIoError::CramReferenceRequired {
                path: path.to_string(),
            })
            .map(Some)
    } else {
        Ok(explicit)
    }
}

pub fn open_reader(
    path: impl AsRef<Path>,
    reference: Option<&str>,
) -> Result<bam::Reader, HtsIoError> {
    open_reader_with_cram_decode_md(path, reference, false)
}

pub fn open_reader_with_cram_decode_md(
    path: impl AsRef<Path>,
    reference: Option<&str>,
    decode_md: bool,
) -> Result<bam::Reader, HtsIoError> {
    let path = path.as_ref();
    let path_text = path.to_string_lossy();
    let reference = resolve_reference_sequence(&path_text, reference)?;
    let mut reader = bam::Reader::from_path(path)?;
    if let Some(reference) = reference.as_deref() {
        reader.set_reference(reference)?;
    }
    configure_cram_decode(&mut reader, &path_text, decode_md)?;
    configure_reader_threads(&mut reader)?;
    Ok(reader)
}

pub fn open_reader_pipelined(
    path: impl AsRef<Path>,
    reference: Option<&str>,
) -> Result<bam::Reader, HtsIoError> {
    let path = path.as_ref();
    let path_text = path.to_string_lossy();
    let reference = resolve_reference_sequence(&path_text, reference)?;
    let mut reader = bam::Reader::from_path(path)?;
    if let Some(reference) = reference.as_deref() {
        reader.set_reference(reference)?;
    }
    configure_cram_decode(&mut reader, &path_text, false)?;
    if let Some(threads) = bgzf_threads_for(HtsThreadRole::PipelineReader) {
        reader.set_threads(threads)?;
    }
    Ok(reader)
}

pub fn open_writer(
    output: &str,
    header: &bam::Header,
    format: Format,
    reference: Option<&str>,
    compression_level: Option<u32>,
) -> Result<bam::Writer, HtsIoError> {
    let reference = resolve_reference_sequence(output, reference)?;
    let mut writer = bam::Writer::from_path(output, header, format)?;
    if let Some(reference) = reference.as_deref() {
        writer.set_reference(reference)?;
    }
    if matches!(format, Format::Bam | Format::Cram) {
        configure_writer_threads(&mut writer)?;
    }
    if let Some(level) = compression_level {
        writer.set_compression_level(bam::CompressionLevel::Level(level))?;
    }
    Ok(writer)
}

fn configure_cram_decode(
    reader: &mut bam::Reader,
    path: &str,
    decode_md: bool,
) -> Result<(), HtsIoError> {
    if path_format(path) == Some(Format::Cram) {
        let decode_md = u32::from(decode_md);
        reader.set_cram_options(htslib::hts_fmt_option_CRAM_OPT_DECODE_MD, decode_md)?;
    }
    Ok(())
}

pub fn configure_reader_threads(reader: &mut bam::Reader) -> Result<(), HtsIoError> {
    if let Some(threads) = bgzf_threads_for(HtsThreadRole::Reader) {
        reader.set_threads(threads)?;
    }
    Ok(())
}

pub fn configure_writer_threads(writer: &mut bam::Writer) -> Result<(), HtsIoError> {
    if let Some(threads) = bgzf_threads_for(HtsThreadRole::Writer) {
        writer.set_threads(threads)?;
    }
    Ok(())
}

pub fn writer_format_for_output(output: &str) -> Result<Format, HtsIoError> {
    output_format_for_path(output, "output")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_quality_sentinel_is_not_a_phred_measurement() {
        let mut record = bam::Record::new();
        record.set(b"read", None, b"ACGT", &[255; 4]);
        assert_eq!(record.seq_len(), 4);
        assert_eq!(base_qualities(&record), []);
        record.set(b"read", None, b"ACGT", &[0, 15, 30, 40]);
        assert_eq!(base_qualities(&record), &[0, 15, 30, 40]);
    }

    #[test]
    fn detects_supported_extensions() {
        assert_eq!(path_format("reads.bam"), Some(Format::Bam));
        assert_eq!(path_format("reads.cram"), Some(Format::Cram));
        assert_eq!(path_format("reads.sam"), Some(Format::Sam));
        assert!(path_format("reads.gz").is_none());
    }

    #[test]
    fn cram_requires_reference() {
        let error = resolve_reference_sequence("shard.cram", None).unwrap_err();
        assert!(error.to_string().contains("REFERENCE_SEQUENCE"));
    }

    #[test]
    fn bam_does_not_require_reference() {
        assert_eq!(resolve_reference_sequence("shard.bam", None).unwrap(), None);
    }
}
