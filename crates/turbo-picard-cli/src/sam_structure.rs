use super::parse_cigar_text;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(super) enum SamStructureError {
    #[error("ValidateSamFile sequence header is missing SN")]
    MissingSequenceName,
    #[error("ValidateSamFile sequence header is missing LN")]
    MissingSequenceLength,
    #[error("ValidateSamFile invalid sequence length")]
    InvalidSequenceLength,
    #[error("ValidateSamFile CIGAR length overflow")]
    CigarLengthOverflow,
    #[error("{0}")]
    Cigar(String),
}

#[derive(Default)]
pub(super) struct Structure {
    pub targets: BTreeMap<String, (usize, u64)>,
    coordinate_sorted: bool,
    previous: Option<(usize, u64)>,
}

impl Structure {
    pub fn header(&mut self, line: &str) -> Result<(), SamStructureError> {
        let fields: BTreeMap<_, _> = line
            .trim_end()
            .split('\t')
            .skip(1)
            .filter_map(|field| field.split_once(':'))
            .collect();
        if line.starts_with("@HD\t") {
            self.coordinate_sorted = fields.get("SO") == Some(&"coordinate");
        } else if line.starts_with("@SQ\t") {
            let name = fields
                .get("SN")
                .ok_or(SamStructureError::MissingSequenceName)?;
            let length = fields
                .get("LN")
                .ok_or(SamStructureError::MissingSequenceLength)?
                .parse::<u64>()
                .map_err(|_| SamStructureError::InvalidSequenceLength)?;
            self.targets
                .insert((*name).to_string(), (self.targets.len(), length));
        }
        Ok(())
    }

    pub fn observe(
        &mut self,
        flags: u16,
        reference: &str,
        position: u64,
        cigar: &str,
        sequence_length: usize,
        quality_length: Option<usize>,
    ) -> Result<Vec<&'static str>, SamStructureError> {
        let mut issues = Vec::new();
        if quality_length.is_some_and(|length| length != sequence_length) {
            issues.push("ERROR:MISMATCH_SEQ_QUAL_LENGTH");
        }
        let target = self.targets.get(reference).copied();
        let coordinate = (target.map_or(usize::MAX, |(index, _)| index), position);
        if self.coordinate_sorted && self.previous.is_some_and(|previous| coordinate < previous) {
            issues.push("ERROR:RECORD_OUT_OF_ORDER");
        }
        self.previous = Some(coordinate);
        if flags & 0x4 == 0 {
            if position == 0 || target.is_some_and(|(_, length)| position > length) {
                issues.push("ERROR:INVALID_ALIGNMENT_START");
            }
            if target.is_none() {
                issues.push("ERROR:MISSING_SEQUENCE_DICTIONARY");
            }
            let operations = parse_cigar_text(cigar).map_err(SamStructureError::Cigar)?;
            let mut read_length = 0_u64;
            let mut reference_length = 0_u64;
            for (length, operation) in operations {
                if matches!(operation, 'M' | 'I' | 'S' | '=' | 'X') {
                    read_length = read_length
                        .checked_add(length)
                        .ok_or(SamStructureError::CigarLengthOverflow)?;
                }
                if matches!(operation, 'M' | 'D' | 'N' | '=' | 'X') {
                    reference_length = reference_length
                        .checked_add(length)
                        .ok_or(SamStructureError::CigarLengthOverflow)?;
                }
            }
            if read_length != sequence_length as u64 {
                issues.push("ERROR:MISMATCH_CIGAR_SEQ_LENGTH");
            }
            if target.is_some_and(|(_, length)| {
                position.saturating_sub(1).saturating_add(reference_length) > length
            }) {
                issues.push("ERROR:CIGAR_MAPS_OFF_REFERENCE");
            }
        }
        Ok(issues)
    }
}
