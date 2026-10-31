use crate::{
    MarkDuplicatesConfig, MarkDuplicatesError, external_plan_tempdir, open_markdup_reader,
};
use rust_htslib::bam::record::Aux;
use rust_htslib::bam::{self, Read};
use std::collections::{BTreeMap, BTreeSet};
use tempfile::TempDir;
use turbo_picard_core::hts_io;

pub(super) struct PreparedInputs {
    pub paths: Vec<String>,
    temporary: Option<TempDir>,
}

type Attributes = BTreeMap<String, String>;

pub(super) fn prepare(
    config: &MarkDuplicatesConfig,
) -> Result<PreparedInputs, MarkDuplicatesError> {
    let mut prepared = PreparedInputs {
        paths: config.inputs.clone(),
        temporary: None,
    };
    if config.inputs.len() == 1 {
        return Ok(prepared);
    }
    let mut headers = Vec::new();
    let mut dictionary = None;
    let mut checksums = BTreeMap::<String, String>::new();
    let mut reserved = BTreeSet::new();
    for input in &config.inputs {
        let reader = open_markdup_reader(config, input)?;
        let current: Vec<_> = reader
            .header()
            .target_names()
            .iter()
            .zip(0_u32..)
            .map(|(name, tid)| (name.to_vec(), reader.header().target_len(tid)))
            .collect();
        if dictionary
            .as_ref()
            .is_some_and(|expected| expected != &current)
        {
            return Err(MarkDuplicatesError::SequenceDictionariesDiffer {
                input: input.clone(),
            });
        }
        dictionary = Some(current);
        let header = String::from_utf8_lossy(reader.header().as_bytes()).into_owned();
        for line in header.lines().filter(|line| line.starts_with("@SQ\t")) {
            let fields: BTreeMap<_, _> = line
                .split('\t')
                .skip(1)
                .filter_map(|field| field.split_once(':'))
                .collect();
            if let (Some(name), Some(checksum)) = (fields.get("SN"), fields.get("M5")) {
                let checksum = checksum.to_ascii_lowercase();
                if checksums
                    .get(*name)
                    .is_some_and(|expected| expected != &checksum)
                {
                    return Err(MarkDuplicatesError::ConflictingSequenceChecksum {
                        name: (*name).to_string(),
                        input: input.clone(),
                    });
                }
                checksums.insert(name.to_string(), checksum);
            }
        }
        for fields in read_groups(&header)? {
            reserved.insert(fields["ID"].clone());
        }
        headers.push(header);
    }

    let mut definitions = BTreeMap::<Attributes, String>::new();
    let mut assigned = BTreeSet::new();
    let mut plans = Vec::new();
    for header in &headers {
        let mut renames = BTreeMap::new();
        for fields in read_groups(header)? {
            let old = fields["ID"].clone();
            let new = if let Some(id) = definitions.get(&fields) {
                id.clone()
            } else {
                let mut id = old.clone();
                if assigned.contains(&old) {
                    for suffix in 1_u64.. {
                        id = format!("{old}.{suffix}");
                        if !reserved.contains(&id) {
                            break;
                        }
                    }
                }
                reserved.insert(id.clone());
                assigned.insert(id.clone());
                definitions.insert(fields, id.clone());
                id
            };
            if old != new {
                renames.insert(old, new);
            }
        }
        plans.push(renames);
    }

    for (index, renames) in plans.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
        let temporary = match &prepared.temporary {
            Some(temporary) => temporary,
            None => prepared.temporary.insert(external_plan_tempdir(config)?),
        };
        let path = temporary.path().join(format!("input-{index}.bam"));
        let mut text = String::new();
        for line in headers[index].lines() {
            for (field_index, field) in line.split('\t').enumerate() {
                if field_index > 0 {
                    text.push('\t');
                }
                if line.starts_with("@RG\t")
                    && let Some(id) = field.strip_prefix("ID:")
                    && let Some(new) = renames.get(id)
                {
                    text.push_str("ID:");
                    text.push_str(new);
                } else {
                    text.push_str(field);
                }
            }
            text.push('\n');
        }
        let header = bam::Header::from_template(&bam::HeaderView::from_bytes(text.as_bytes()));
        let mut reader = open_markdup_reader(config, &config.inputs[index])?;
        let mut writer = hts_io::open_writer(
            path.to_str()
                .ok_or(MarkDuplicatesError::NonUtf8TemporaryPath)?,
            &header,
            bam::Format::Bam,
            None,
            Some(0),
        )?;
        for record in reader.records() {
            let mut record = record?;
            if let Ok(Aux::String(old)) = record.aux(b"RG")
                && let Some(new) = renames.get(old)
            {
                record.remove_aux(b"RG")?;
                record.push_aux(b"RG", Aux::String(new))?;
            }
            writer.write(&record)?;
        }
        drop(writer);
        prepared.paths[index] = path.to_string_lossy().into_owned();
    }
    Ok(prepared)
}

fn read_groups(header: &str) -> Result<Vec<Attributes>, MarkDuplicatesError> {
    let mut groups = BTreeMap::new();
    for line in header.lines().filter(|line| line.starts_with("@RG\t")) {
        let fields: Attributes = line
            .split('\t')
            .skip(1)
            .filter_map(|field| field.split_once(':'))
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        let id = fields
            .get("ID")
            .ok_or(MarkDuplicatesError::ReadGroupMissingId)?
            .clone();
        if groups.insert(id.clone(), fields).is_some() {
            return Err(MarkDuplicatesError::DuplicateReadGroupId { id });
        }
    }
    Ok(groups.into_values().collect())
}
