//! SAM/BAM/CRAM path helpers shared by native commands.

use crate::bgzf_threads::bgzf_threads;
use rust_htslib::bam::{self, Format, Read};
use std::path::Path;

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
    matches!(
        path_extension_lower(path).as_deref(),
        Some("bam") | Some("cram")
    )
}

pub fn is_sam_text_input(path: &str) -> bool {
    path_extension_lower(path).as_deref() == Some("sam")
}

pub fn output_format_for_path(output: &str, command: &str) -> Result<Format, String> {
    path_format(output).ok_or_else(|| {
        format!("unsupported {command} output format for {output}; expected .sam, .bam, or .cram")
    })
}

pub fn resolve_reference_sequence(
    path: &str,
    explicit: Option<&str>,
) -> Result<Option<String>, String> {
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
            .ok_or_else(|| {
                format!(
                    "CRAM path {path} requires REFERENCE_SEQUENCE (R=) or TURBO_PICARD_REFERENCE"
                )
            })
            .map(Some)
    } else {
        Ok(explicit)
    }
}

pub fn open_reader(path: impl AsRef<Path>, reference: Option<&str>) -> Result<bam::Reader, String> {
    let path = path.as_ref();
    let path_text = path.to_string_lossy();
    let reference = resolve_reference_sequence(&path_text, reference)?;
    let mut reader = bam::Reader::from_path(path).map_err(|error| error.to_string())?;
    if let Some(reference) = reference.as_deref() {
        reader
            .set_reference(reference)
            .map_err(|error| error.to_string())?;
    }
    configure_reader_threads(&mut reader)?;
    Ok(reader)
}

/// Open a reader for pipelined decode (dedicated reader thread overlaps I/O).
///
/// BGZF worker threads are pinned to 1 so decode does not fight the
/// application reader thread for the same block pool.
pub fn open_reader_pipelined(
    path: impl AsRef<Path>,
    reference: Option<&str>,
) -> Result<bam::Reader, String> {
    let path = path.as_ref();
    let path_text = path.to_string_lossy();
    let reference = resolve_reference_sequence(&path_text, reference)?;
    let mut reader = bam::Reader::from_path(path).map_err(|error| error.to_string())?;
    if let Some(reference) = reference.as_deref() {
        reader
            .set_reference(reference)
            .map_err(|error| error.to_string())?;
    }
    reader
        .set_threads(1)
        .map_err(|error| error.to_string())?;
    Ok(reader)
}

pub fn open_writer(
    output: &str,
    header: &bam::Header,
    format: Format,
    reference: Option<&str>,
    compression_level: Option<u32>,
) -> Result<bam::Writer, String> {
    let reference = resolve_reference_sequence(output, reference)?;
    let mut writer =
        bam::Writer::from_path(output, header, format).map_err(|error| error.to_string())?;
    if let Some(reference) = reference.as_deref() {
        writer
            .set_reference(reference)
            .map_err(|error| error.to_string())?;
    }
    if matches!(format, Format::Bam | Format::Cram) {
        configure_writer_threads(&mut writer)?;
    }
    if let Some(level) = compression_level {
        writer
            .set_compression_level(bam::CompressionLevel::Level(level))
            .map_err(|error| error.to_string())?;
    }
    Ok(writer)
}

pub fn configure_reader_threads(reader: &mut bam::Reader) -> Result<(), String> {
    if let Some(threads) = bgzf_threads() {
        reader
            .set_threads(threads)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn configure_writer_threads(writer: &mut bam::Writer) -> Result<(), String> {
    if let Some(threads) = bgzf_threads() {
        writer
            .set_threads(threads)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn writer_format_for_output(output: &str) -> Result<Format, String> {
    output_format_for_path(output, "output")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(error.contains("REFERENCE_SEQUENCE"));
    }

    #[test]
    fn bam_does_not_require_reference() {
        assert_eq!(resolve_reference_sequence("shard.bam", None).unwrap(), None);
    }
}
