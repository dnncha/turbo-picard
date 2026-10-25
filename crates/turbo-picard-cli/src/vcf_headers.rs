//! Shared VCF metadata union and gather preconditions.
use super::*;

// Metadata values may contain quoted commas and escaped quotes. Splitting on
// commas without this framing corrupts Description and other structured fields.
fn attributes(value: &str) -> Result<Vec<(&str, &str)>, String> {
    let body = value
        .strip_prefix('<')
        .and_then(|v| v.strip_suffix('>'))
        .ok_or_else(|| "malformed structured VCF header".to_string())?;
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
                    .ok_or_else(|| "malformed VCF header attribute".to_string())?,
            );
            start = index + 1;
        }
    }
    if quoted || escaped {
        return Err("unterminated VCF header quote".into());
    }
    fields.push(
        body[start..]
            .split_once('=')
            .ok_or_else(|| "malformed VCF header attribute".to_string())?,
    );
    Ok(fields)
}

fn key(line: &str) -> Result<(String, Option<String>), String> {
    let (kind, value) = line
        .strip_prefix("##")
        .and_then(|line| line.split_once('='))
        .ok_or_else(|| "malformed VCF metadata line".to_string())?;
    let id = if value.starts_with('<') {
        attributes(value)?
            .iter()
            .find(|(key, _)| *key == "ID")
            .map(|(_, value)| value.to_string())
    } else {
        None
    };
    Ok((kind.to_string(), id))
}

pub(super) fn merge(documents: &[VcfDocument]) -> Result<VcfDocument, String> {
    let mut output = documents
        .first()
        .ok_or_else(|| "missing VCF inputs".to_string())?
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
                    return Err("incompatible VCF header versions".into());
                }
            }
            if let Some(&index) = indices.get(&identity) {
                if matches!(identity.0.as_str(), "INFO" | "FORMAT") {
                    let existing: &mut String = &mut output.meta_lines[index];
                    let old = attributes(existing.split_once('=').unwrap().1)?;
                    let new = attributes(line.split_once('=').unwrap().1)?;
                    let get = |fields: &[(&str, &str)], name: &str| {
                        fields
                            .iter()
                            .find(|(key, _)| *key == name)
                            .map(|(_, value)| value.to_string())
                    };
                    if get(&old, "Type") != get(&new, "Type") {
                        return Err(format!(
                            "unsupported VCF header Type conflict for {} {}",
                            identity.0,
                            identity.1.as_deref().unwrap_or("")
                        ));
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

pub(super) fn validate_gather(inputs: &[String], header: &VcfDocument) -> Result<(), String> {
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
            let contig = order.get(&record.contig).ok_or_else(|| {
                format!(
                    "GatherVcfs contig {} is missing from sequence dictionary",
                    record.contig
                )
            })?;
            let current = (*contig, record.position);
            if previous.is_some_and(|previous| current < previous || (first && current == previous))
            {
                return Err(format!(
                    "GatherVcfs inputs are out of order or overlap at {}:{} in {input}",
                    record.contig, record.position
                ));
            }
            previous = Some(current);
            first = false;
            let fields: Vec<_> = record.line.split('\t').collect();
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
                    return Err(format!(
                        "GatherVcfs {kind} {id} is not defined in the output header"
                    ));
                }
            }
            Ok(())
        })?;
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
