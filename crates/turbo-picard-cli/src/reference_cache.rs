use super::read_text_or_gzip;
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

pub(crate) const MAX_PRELOAD_BASES: usize = 256_000_000;

pub(crate) type PreloadedContigs = Arc<Vec<Option<Vec<u8>>>>;

pub(crate) struct FastaSequence {
    pub(crate) name: String,
    pub(crate) sequence: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct FaiEntry {
    length: usize,
    offset: u64,
}

pub(crate) fn read_reference_contigs(path: &str) -> Result<Vec<(String, usize)>, String> {
    let fai_path = format!("{path}.fai");
    if Path::new(&fai_path).is_file() {
        return read_fai_contig_lengths(&fai_path);
    }
    read_fasta_contig_lengths(path, true)
}

pub(crate) fn load_fasta_contig_sequence(path: &str, contig: &str) -> Result<Vec<u8>, String> {
    let fai_path = format!("{path}.fai");
    if Path::new(&fai_path).is_file() {
        let entry = read_fai_entry(&fai_path, contig)?;
        return load_fasta_contig_sequence_indexed(path, contig, entry);
    }
    load_fasta_contig_sequence_scan(path, contig)
}

pub(crate) fn read_fasta_sequences(
    path: &str,
    truncate_names: bool,
) -> Result<Vec<FastaSequence>, String> {
    let text = read_text_or_gzip(path)?;
    let mut records = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_sequence = Vec::new();

    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                records.push(FastaSequence {
                    name,
                    sequence: std::mem::take(&mut current_sequence),
                });
            }
            let name = if truncate_names {
                header.split_whitespace().next().unwrap_or_default()
            } else {
                header
            };
            if name.is_empty() {
                return Err("empty FASTA sequence name".to_string());
            }
            current_name = Some(name.to_string());
        } else if current_name.is_some() {
            current_sequence.extend(line.trim().as_bytes().iter().map(u8::to_ascii_uppercase));
        } else if !line.trim().is_empty() {
            return Err("FASTA sequence data before first header".to_string());
        }
    }

    if let Some(name) = current_name {
        records.push(FastaSequence {
            name,
            sequence: current_sequence,
        });
    }
    if records.is_empty() {
        return Err("FASTA contains no sequences".to_string());
    }
    Ok(records)
}

pub(crate) fn reference_sequences_by_name(path: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    Ok(read_fasta_sequences(path, true)?
        .into_iter()
        .map(|record| (record.name, record.sequence.to_ascii_uppercase()))
        .collect())
}

pub(crate) fn preload_reference_contigs_for_tids(
    reference_path: &str,
    target_names: &[String],
) -> Result<Option<PreloadedContigs>, String> {
    let contigs = read_reference_contigs(reference_path)?;
    let total_bases = contigs.iter().map(|(_, len)| *len).sum::<usize>();
    if total_bases > MAX_PRELOAD_BASES {
        return Ok(None);
    }
    let needed: BTreeSet<&str> = target_names.iter().map(String::as_str).collect();
    let mut by_name = FxHashMap::default();
    for fasta in read_fasta_sequences(reference_path, true)? {
        if needed.contains(fasta.name.as_str()) {
            by_name.insert(fasta.name, fasta.sequence);
        }
    }
    let slots = target_names
        .iter()
        .map(|name| by_name.remove(name))
        .collect::<Vec<_>>();
    Ok(Some(Arc::new(slots)))
}

#[derive(Debug)]
pub(crate) struct ContigReferenceCache {
    reference_path: String,
    active_contig: Option<String>,
    active_sequence: Vec<u8>,
    preloaded: Option<PreloadedContigs>,
}

impl ContigReferenceCache {
    pub(crate) fn new(reference_path: &str) -> Self {
        Self::new_with_preloaded(reference_path, None)
    }

    pub(crate) fn new_with_preloaded(
        reference_path: &str,
        preloaded: Option<PreloadedContigs>,
    ) -> Self {
        Self {
            reference_path: reference_path.to_string(),
            active_contig: None,
            active_sequence: Vec::new(),
            preloaded,
        }
    }

    pub(crate) fn contig_sequence(
        &mut self,
        tid_idx: usize,
        target_names: &[String],
        command_name: &str,
    ) -> Result<&[u8], String> {
        if let Some(preloaded) = &self.preloaded
            && let Some(Some(sequence)) = preloaded.get(tid_idx)
        {
            return Ok(sequence.as_slice());
        }
        let contig = target_names
            .get(tid_idx)
            .ok_or_else(|| format!("{command_name} record references unknown target"))?;
        if self.active_contig.as_deref() != Some(contig.as_str()) {
            self.active_sequence = load_fasta_contig_sequence(&self.reference_path, contig)?;
            self.active_contig = Some(contig.clone());
        }
        Ok(self.active_sequence.as_slice())
    }
}

fn read_fai_contig_lengths(path: &str) -> Result<Vec<(String, usize)>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut contigs = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(length_text) = fields.next() else {
            return Err(format!("invalid FAI entry in {path}"));
        };
        let length = length_text
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI length in {path}: {error}"))?;
        contigs.push((name.to_string(), length));
    }
    if contigs.is_empty() {
        return Err(format!("FAI index {path} contains no contigs"));
    }
    Ok(contigs)
}

fn read_fasta_contig_lengths(
    path: &str,
    truncate_names: bool,
) -> Result<Vec<(String, usize)>, String> {
    let text = read_text_or_gzip(path)?;
    let mut contigs = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_length = 0usize;

    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                contigs.push((name, current_length));
            }
            current_length = 0;
            let name = if truncate_names {
                header.split_whitespace().next().unwrap_or_default()
            } else {
                header
            };
            if name.is_empty() {
                return Err("empty FASTA sequence name".to_string());
            }
            current_name = Some(name.to_string());
        } else if current_name.is_some() {
            current_length += line.trim().len();
        } else if !line.trim().is_empty() {
            return Err("FASTA sequence data before first header".to_string());
        }
    }

    if let Some(name) = current_name {
        contigs.push((name, current_length));
    }
    if contigs.is_empty() {
        return Err("FASTA contains no sequences".to_string());
    }
    Ok(contigs)
}

fn read_fai_entry(fai_path: &str, contig: &str) -> Result<FaiEntry, String> {
    let text = fs::read_to_string(fai_path).map_err(|error| error.to_string())?;
    for line in text.lines() {
        let mut fields = line.split('\t');
        let Some(name) = fields.next() else {
            continue;
        };
        if name != contig {
            continue;
        }
        let length = fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI length for {contig}: {error}"))?;
        let offset = fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<u64>()
            .map_err(|error| format!("invalid FAI offset for {contig}: {error}"))?;
        fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI bases-per-line for {contig}: {error}"))?;
        fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI line width for {contig}: {error}"))?;
        return Ok(FaiEntry { length, offset });
    }
    Err(format!("FAI index {fai_path} missing contig {contig}"))
}

fn load_fasta_contig_sequence_scan(path: &str, contig: &str) -> Result<Vec<u8>, String> {
    let text = read_text_or_gzip(path)?;
    let mut current_name: Option<String> = None;
    let mut sequence = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                if name == contig {
                    return Ok(sequence);
                }
                sequence.clear();
            }
            let name = header.split_whitespace().next().unwrap_or_default();
            if name == contig {
                current_name = Some(name.to_string());
            }
        } else if current_name.is_some() {
            sequence.extend(line.trim().as_bytes().iter().map(u8::to_ascii_uppercase));
        }
    }
    if current_name.as_deref() == Some(contig) {
        return Ok(sequence);
    }
    Err(format!("FASTA {path} missing contig {contig}"))
}

fn load_fasta_contig_sequence_indexed(
    path: &str,
    contig: &str,
    entry: FaiEntry,
) -> Result<Vec<u8>, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(entry.offset))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut sequence = Vec::with_capacity(entry.length);
    let mut remaining = entry.length;
    while remaining > 0 {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('>') {
            continue;
        }
        let take = remaining.min(line.len());
        sequence.extend(line.as_bytes()[..take].iter().map(u8::to_ascii_uppercase));
        remaining -= take;
    }
    if sequence.len() != entry.length {
        return Err(format!(
            "FASTA {path} contig {contig} truncated: expected {} bases, read {}",
            entry.length,
            sequence.len()
        ));
    }
    Ok(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contig_reference_cache_resolves_preloaded_and_on_demand_contigs() {
        let dir = tempfile::tempdir().unwrap();
        let fasta = dir.path().join("ref.fa");
        fs::write(&fasta, ">chr1\nacgt\n>chr2\nTTAA\n").unwrap();
        let fasta_str = fasta.to_str().unwrap();
        let targets = vec!["chr1".to_string(), "chr2".to_string()];

        let preloaded = preload_reference_contigs_for_tids(fasta_str, &targets).unwrap();
        assert!(preloaded.is_some());
        let mut cached = ContigReferenceCache::new_with_preloaded(fasta_str, preloaded);
        assert_eq!(
            cached.contig_sequence(0, &targets, "Test").unwrap(),
            b"ACGT"
        );
        assert_eq!(
            cached.contig_sequence(1, &targets, "Test").unwrap(),
            b"TTAA"
        );

        let mut on_demand = ContigReferenceCache::new(fasta_str);
        assert_eq!(
            on_demand.contig_sequence(1, &targets, "Test").unwrap(),
            b"TTAA"
        );
        assert!(on_demand.contig_sequence(5, &targets, "Test").is_err());
    }
}
