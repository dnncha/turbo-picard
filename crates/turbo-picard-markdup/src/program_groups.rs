use std::collections::HashSet;

use rust_htslib::bam::{self, header::HeaderRecord, record::Aux};

pub(crate) const PROGRAM_RECORD_ID: &str = "MarkDuplicates";

#[derive(Debug, Default)]
pub(crate) struct ProgramGroupsSeen {
    order: Vec<Option<Vec<u8>>>,
    seen: HashSet<Option<Vec<u8>>>,
}

impl ProgramGroupsSeen {
    pub(crate) fn observe(&mut self, record: &bam::Record) {
        let value = match record.aux(b"PG") {
            Ok(Aux::String(value)) => Some(value.as_bytes()),
            Ok(Aux::Char(value)) => {
                return self.observe_value(Some(&[value]));
            }
            _ => None,
        };
        self.observe_value(value);
    }

    pub(crate) fn observe_value(&mut self, value: Option<&[u8]>) {
        let already = match value {
            Some(bytes) => self.seen.contains(&Some(bytes.to_vec())),
            None => self.seen.contains(&None),
        };
        if !already {
            let owned = value.map(<[u8]>::to_vec);
            self.seen.insert(owned.clone());
            self.order.push(owned);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProgramGroups {
    chained: Vec<(Option<Vec<u8>>, String)>,
}

impl ProgramGroups {
    pub(crate) fn chain(
        header: &mut bam::Header,
        seen: &ProgramGroupsSeen,
        version: &str,
        command_line: &str,
    ) -> Self {
        let existing_ids = header_program_ids(header);
        let mut generator = ProgramIdGenerator::new(&existing_ids);
        let mut chained = Vec::with_capacity(seen.order.len());
        for index in java_hash_set_iteration_order(&seen.order) {
            let previous = seen.order[index].clone();
            let id = generator.non_colliding(PROGRAM_RECORD_ID);
            let mut record = HeaderRecord::new(b"PG");
            record.push_tag(b"ID", id.as_str());
            record.push_tag(b"VN", version);
            record.push_tag(b"CL", command_line);
            record.push_tag(b"PN", PROGRAM_RECORD_ID);
            if let Some(previous) = previous.as_deref() {
                record.push_tag(b"PP", String::from_utf8_lossy(previous).as_ref());
            }
            header.push_record(&record);
            chained.push((previous, id));
        }
        Self { chained }
    }

    pub(crate) fn id_for(&self, existing: Option<&[u8]>) -> Option<&str> {
        self.chained
            .iter()
            .find(|(previous, _)| previous.as_deref() == existing)
            .map(|(_, id)| id.as_str())
    }

    pub(crate) fn retag(&self, record: &mut bam::Record) -> Result<(), rust_htslib::errors::Error> {
        let existing = match record.aux(b"PG") {
            Ok(Aux::String(value)) => Some(value.as_bytes().to_vec()),
            Ok(Aux::Char(value)) => Some(vec![value]),
            _ => None,
        };
        let Some(id) = self.id_for(existing.as_deref()).map(str::to_string) else {
            return Ok(());
        };
        if existing.is_some() {
            record.remove_aux(b"PG")?;
        }
        record.push_aux(b"PG", Aux::String(&id))
    }
}

fn header_program_ids(header: &bam::Header) -> Vec<String> {
    let bytes = header.to_bytes();
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|line| line.starts_with("@PG\t"))
        .filter_map(|line| {
            line.split('\t')
                .find_map(|field| field.strip_prefix("ID:"))
                .map(str::to_string)
        })
        .collect()
}

struct ProgramIdGenerator {
    taken: HashSet<String>,
    counter: u64,
}

impl ProgramIdGenerator {
    fn new(existing: &[String]) -> Self {
        Self {
            taken: existing.iter().cloned().collect(),
            counter: u64::try_from(existing.len()).unwrap_or(u64::MAX),
        }
    }

    fn non_colliding(&mut self, base: &str) -> String {
        if self.taken.insert(base.to_string()) {
            self.counter += 1;
            return base.to_string();
        }
        loop {
            let candidate = format!("{base}.{}", positive_base36(self.counter));
            self.counter += 1;
            if self.taken.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

fn positive_base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(DIGITS[usize::try_from(value % 36).unwrap_or(0)]);
        value /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}

fn java_string_hash(bytes: &[u8]) -> i32 {
    String::from_utf8_lossy(bytes)
        .encode_utf16()
        .fold(0_i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        })
}

fn java_hash_set_iteration_order(values: &[Option<Vec<u8>>]) -> Vec<usize> {
    let mut capacity = 16_usize;
    while values.len() * 4 > capacity * 3 {
        capacity *= 2;
    }
    let mut order = (0..values.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        let hash = values[index]
            .as_deref()
            .map_or(0, java_string_hash)
            .cast_unsigned();
        let spread = hash ^ (hash >> 16);
        (
            usize::try_from(spread).unwrap_or(usize::MAX) & (capacity - 1),
            index,
        )
    });
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_with_program_ids(ids: &[&str]) -> bam::Header {
        let mut header = bam::Header::new();
        for id in ids {
            let mut record = HeaderRecord::new(b"PG");
            record.push_tag(b"ID", *id);
            header.push_record(&record);
        }
        header
    }

    #[test]
    fn colliding_program_id_takes_base36_suffix_from_existing_count() {
        let mut ids = (0..15)
            .map(|index| format!("tool{index}"))
            .collect::<Vec<_>>();
        ids.push("MarkDuplicates".to_string());
        let id_refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let mut header = header_with_program_ids(&id_refs);
        let mut seen = ProgramGroupsSeen::default();
        seen.observe_value(Some(b"bwa"));
        let groups = ProgramGroups::chain(&mut header, &seen, "test", "MarkDuplicates");
        assert_eq!(groups.id_for(Some(b"bwa")), Some("MarkDuplicates.G"));
        assert_eq!(groups.id_for(None), None);
        let text = String::from_utf8_lossy(&header.to_bytes()).to_string();
        assert!(text.contains(
            "ID:MarkDuplicates.G\tVN:test\tCL:MarkDuplicates\tPN:MarkDuplicates\tPP:bwa"
        ));
    }

    #[test]
    fn untaken_program_id_is_used_as_is_and_reads_without_pg_chain_to_it() {
        let mut header = header_with_program_ids(&["bwa"]);
        let mut seen = ProgramGroupsSeen::default();
        seen.observe_value(None);
        seen.observe_value(None);
        let groups = ProgramGroups::chain(&mut header, &seen, "test", "cl");
        assert_eq!(groups.id_for(None), Some("MarkDuplicates"));
    }

    #[test]
    fn several_previous_programs_follow_java_hash_set_order() {
        let mut header = header_with_program_ids(&["MarkDuplicates"]);
        let mut seen = ProgramGroupsSeen::default();
        for value in ["zeta", "alpha", "bwa"] {
            seen.observe_value(Some(value.as_bytes()));
        }
        seen.observe_value(None);
        let groups = ProgramGroups::chain(&mut header, &seen, "test", "cl");
        assert_eq!(groups.id_for(Some(b"zeta")), Some("MarkDuplicates.1"));
        assert_eq!(groups.id_for(None), Some("MarkDuplicates.2"));
        assert_eq!(groups.id_for(Some(b"alpha")), Some("MarkDuplicates.3"));
        assert_eq!(groups.id_for(Some(b"bwa")), Some("MarkDuplicates.4"));
    }

    #[test]
    fn java_string_hash_matches_known_values() {
        assert_eq!(java_string_hash(b""), 0);
        assert_eq!(java_string_hash(b"a"), 97);
        assert_eq!(
            java_string_hash(b"MarkDuplicates"),
            "MarkDuplicates"
                .bytes()
                .fold(0_i32, |h, b| h.wrapping_mul(31).wrapping_add(i32::from(b)))
        );
        assert_eq!(java_string_hash(b"hello"), 99_162_322);
    }

    #[test]
    fn base36_matches_picard_suffixes() {
        assert_eq!(positive_base36(0), "0");
        assert_eq!(positive_base36(16), "G");
        assert_eq!(positive_base36(36), "10");
    }
}
