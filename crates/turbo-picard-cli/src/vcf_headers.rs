use super::{VcfDocument, VcfRecord, stream_vcf_records};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(super) enum VcfHeadersError {
    #[error("malformed structured VCF header")]
    MalformedStructuredHeader,
    #[error("malformed VCF header attribute")]
    MalformedAttribute,
    #[error("unterminated VCF header quote")]
    UnterminatedQuote,
    #[error("malformed VCF metadata line")]
    MalformedMetadataLine,
    #[error("missing VCF inputs")]
    MissingInputs,
    #[error("incompatible VCF header versions")]
    IncompatibleVersions,
    #[error("unsupported VCF header Type conflict for {kind} {id}")]
    TypeConflict { kind: String, id: String },
    #[error("GatherVcfs contig {contig} is missing from sequence dictionary")]
    ContigMissing { contig: String },
    #[error("GatherVcfs inputs are out of order or overlap at {contig}:{position} in {input}")]
    OutOfOrder {
        contig: String,
        position: u64,
        input: String,
    },
    #[error("GatherVcfs requires at least 8 VCF columns in {input}, found {found}")]
    TooFewColumns { input: String, found: usize },
    #[error("GatherVcfs {kind} {id} is not defined in the output header")]
    UndefinedHeader { kind: &'static str, id: String },
    #[error("{0}")]
    Stream(String),
}

impl From<VcfHeadersError> for String {
    fn from(error: VcfHeadersError) -> Self {
        error.to_string()
    }
}

fn attributes(value: &str) -> Result<Vec<(&str, &str)>, VcfHeadersError> {
    let body = value
        .strip_prefix('<')
        .and_then(|v| v.strip_suffix('>'))
        .ok_or(VcfHeadersError::MalformedStructuredHeader)?;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let mut fields = Vec::new();
    for (index, byte) in body.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == b'"' {
            quoted = !quoted;
        }
        if byte == b',' && !quoted {
            fields.push(
                body[start..index]
                    .split_once('=')
                    .ok_or(VcfHeadersError::MalformedAttribute)?,
            );
            start = index + 1;
        }
    }
    if quoted || escaped {
        return Err(VcfHeadersError::UnterminatedQuote);
    }
    fields.push(
        body[start..]
            .split_once('=')
            .ok_or(VcfHeadersError::MalformedAttribute)?,
    );
    Ok(fields)
}

fn key(line: &str) -> Result<(String, Option<String>), VcfHeadersError> {
    let (kind, value) = line
        .strip_prefix("##")
        .and_then(|line| line.split_once('='))
        .ok_or(VcfHeadersError::MalformedMetadataLine)?;
    let id = if value.starts_with('<') {
        attributes(value)?
            .iter()
            .find(|(key, _)| *key == "ID")
            .map(|(_, value)| (*value).to_string())
    } else {
        None
    };
    Ok(if kind == "fileformat" || id.is_some() {
        (kind.to_string(), id)
    } else {
        (line.to_string(), None)
    })
}

pub(super) fn merge(documents: &[VcfDocument]) -> Result<VcfDocument, VcfHeadersError> {
    let mut output = documents
        .first()
        .ok_or(VcfHeadersError::MissingInputs)?
        .clone();
    output.meta_lines.clear();
    let mut indices = BTreeMap::new();
    let mut versions = BTreeSet::new();
    for document in documents {
        for line in &document.meta_lines {
            let identity = key(line)?;
            if identity.0 == "fileformat" {
                versions.insert(line.clone());
                if versions.len() > 1 && versions.iter().any(|version| version.ends_with("VCFv4.3"))
                {
                    return Err(VcfHeadersError::IncompatibleVersions);
                }
            }
            if let Some(&index) = indices.get(&identity) {
                if matches!(identity.0.as_str(), "INFO" | "FORMAT") {
                    let existing: &mut String = &mut output.meta_lines[index];
                    let old = attributes(
                        existing
                            .split_once('=')
                            .ok_or(VcfHeadersError::MalformedMetadataLine)?
                            .1,
                    )?;
                    let new = attributes(
                        line.split_once('=')
                            .ok_or(VcfHeadersError::MalformedMetadataLine)?
                            .1,
                    )?;
                    let get = |fields: &[(&str, &str)], name: &str| {
                        fields
                            .iter()
                            .find(|(key, _)| *key == name)
                            .map(|(_, value)| (*value).to_string())
                    };
                    if get(&old, "Type") != get(&new, "Type") {
                        return Err(VcfHeadersError::TypeConflict {
                            kind: identity.0,
                            id: identity.1.unwrap_or_default(),
                        });
                    }
                    if get(&old, "Number") != get(&new, "Number") {
                        let promoted = old
                            .iter()
                            .map(|(key, value)| {
                                format!("{key}={}", if *key == "Number" { "." } else { value })
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        *existing = format!("##{}=<{promoted}>", identity.0);
                    }
                }
            } else {
                indices.insert(identity, output.meta_lines.len());
                output.meta_lines.push(line.clone());
            }
        }
    }
    Ok(output)
}

pub(super) fn validate_gather(
    inputs: &[String],
    header: &VcfDocument,
) -> Result<(), VcfHeadersError> {
    let order = header.contig_order();
    let definitions: BTreeSet<_> = header
        .meta_lines
        .iter()
        .map(|line| key(line))
        .collect::<Result<_, _>>()?;
    let mut previous = None;
    for input in inputs {
        let mut first = true;
        stream_vcf_records(input, |record| {
            validate_gather_record(&order, &definitions, input, &mut previous, first, &record)
                .map_err(String::from)?;
            first = false;
            Ok(())
        })
        .map_err(VcfHeadersError::Stream)?;
    }
    Ok(())
}

fn validate_gather_record(
    order: &BTreeMap<String, usize>,
    definitions: &BTreeSet<(String, Option<String>)>,
    input: &str,
    previous: &mut Option<(usize, u64)>,
    first: bool,
    record: &VcfRecord,
) -> Result<(), VcfHeadersError> {
    let contig = order
        .get(&record.contig)
        .ok_or_else(|| VcfHeadersError::ContigMissing {
            contig: record.contig.clone(),
        })?;
    let current = (*contig, record.position);
    if previous.is_some_and(|previous| current < previous || (first && current == previous)) {
        return Err(VcfHeadersError::OutOfOrder {
            contig: record.contig.clone(),
            position: record.position,
            input: input.to_string(),
        });
    }
    *previous = Some(current);
    let fields: Vec<_> = record.line.split('\t').collect();
    if fields.len() < 8 {
        return Err(VcfHeadersError::TooFewColumns {
            input: input.to_string(),
            found: fields.len(),
        });
    }
    let mut required = Vec::new();
    for filter in fields[6]
        .split(';')
        .filter(|filter| !matches!(*filter, "." | "PASS"))
    {
        required.push(("FILTER", filter));
    }
    for info in fields[7].split(';').filter(|info| *info != ".") {
        required.push(("INFO", info.split_once('=').map_or(info, |(key, _)| key)));
    }
    if fields.len() > 8 {
        for format in fields[8].split(':').filter(|format| *format != ".") {
            required.push(("FORMAT", format));
        }
    }
    for (kind, id) in required {
        if !definitions.contains(&(kind.to_string(), Some(id.to_string()))) {
            return Err(VcfHeadersError::UndefinedHeader {
                kind,
                id: id.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_metadata_preserves_quoted_commas_and_escapes() {
        assert_eq!(
            attributes(r#"<ID=X,Number=1,Type=String,Description="comma, and \"quote\"">"#)
                .unwrap()[3],
            ("Description", r#""comma, and \"quote\"""#)
        );
        assert!(attributes("<ID=X,Description=\"broken>").is_err());
    }
}
