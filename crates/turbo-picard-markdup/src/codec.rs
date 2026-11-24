use crate::external_mates::PendingMate;
use crate::{
    ExternalPlanError, FIRST_IN_PAIR_FLAG, LibraryId, PAIRED_FLAG, bam_tag_value, bounded_plan,
};
use rust_htslib::bam;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;

pub(crate) const EXTERNAL_DECISION_OPTICAL: u8 = 0x01;
pub(crate) const EXTERNAL_DECISION_DUPLICATE: u8 = 0x02;
pub(crate) const EXTERNAL_DECISION_SET_MEMBERS: u8 = 0x04;
pub(crate) const EXTERNAL_DECISION_PAYLOAD_BYTES: usize = 1 + (2 * size_of::<i32>());
const EXTERNAL_DECISION_ABSENT_TAG: i32 = i32::MIN;
pub(crate) const COMPACT_PAIR_HEADER_LEN: usize = 41;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExternalDecision {
    pub(crate) flags: u8,
    pub(crate) duplicate_set_size: Option<i32>,
    pub(crate) duplicate_set_index: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalBarcodeValues {
    pub primary: Option<Vec<u8>>,
    pub read_one: Option<Vec<u8>>,
    pub read_two: Option<Vec<u8>>,
}

impl ExternalBarcodeValues {
    pub(crate) const fn empty() -> Self {
        Self {
            primary: None,
            read_one: None,
            read_two: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalPlanRecord {
    pub(crate) ordinal: u64,
    pub(crate) library_id: LibraryId,
    pub(crate) read_group: Option<Vec<u8>>,
    pub(crate) template_key: Vec<u8>,
    pub(crate) flags: u16,
    pub(crate) reference_id: i32,
    pub(crate) position: i64,
    pub(crate) mate_reference_id: i32,
    pub(crate) mate_position: i64,
    pub(crate) template_length: i64,
    pub(crate) unclipped_position: i64,
    pub(crate) quality_score: u64,
    pub(crate) qname: Vec<u8>,
    pub(crate) barcode: ExternalBarcodeValues,
}

pub(crate) struct BorrowedExternalPlanRecord<'a> {
    pub(crate) ordinal: u64,
    pub(crate) library_id: LibraryId,
    pub(crate) read_group: Option<&'a [u8]>,
    pub(crate) flags: u16,
    pub(crate) reference_id: i32,
    pub(crate) position: i64,
    pub(crate) mate_reference_id: i32,
    pub(crate) mate_position: i64,
    pub(crate) template_length: i64,
    pub(crate) unclipped_position: i64,
    pub(crate) quality_score: u64,
    pub(crate) qname: &'a [u8],
    pub(crate) barcode: [Option<&'a [u8]>; 3],
}

impl BorrowedExternalPlanRecord<'_> {
    pub(crate) fn into_owned(self) -> ExternalPlanRecord {
        ExternalPlanRecord {
            ordinal: self.ordinal,
            library_id: self.library_id,
            read_group: self.read_group.map(<[u8]>::to_vec),
            flags: self.flags,
            reference_id: self.reference_id,
            position: self.position,
            mate_reference_id: self.mate_reference_id,
            mate_position: self.mate_position,
            template_length: self.template_length,
            unclipped_position: self.unclipped_position,
            quality_score: self.quality_score,
            qname: self.qname.to_vec(),
            template_key: bounded_plan::template_key(self.read_group, self.qname),
            barcode: ExternalBarcodeValues {
                primary: self.barcode[0].map(<[u8]>::to_vec),
                read_one: self.barcode[1].map(<[u8]>::to_vec),
                read_two: self.barcode[2].map(<[u8]>::to_vec),
            },
        }
    }
}

pub(crate) struct CompactPairHeader<'a> {
    pub(crate) first_ordinal: u64,
    pub(crate) second_ordinal: u64,
    pub(crate) first_flags: u16,
    pub(crate) second_flags: u16,
    pub(crate) library_id: LibraryId,
    pub(crate) rg_id: u32,
    pub(crate) combined_score: u64,
    pub(crate) first_is_left: bool,
    pub(crate) qname: &'a [u8],
    pub(crate) barcodes_tail: &'a [u8],
}

struct ExternalKey<'a> {
    library_id: LibraryId,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    orientation: u8,
    reverse_strand: bool,
    barcode: &'a ExternalBarcodeValues,
}

pub(crate) fn external_barcode_values(
    record: &bam::Record,
    config: &MarkDuplicatesConfig,
) -> ExternalBarcodeValues {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return ExternalBarcodeValues {
            primary: bam_tag_value(record, tag),
            read_one: None,
            read_two: None,
        };
    }

    let paired = record.flags() & PAIRED_FLAG != 0;
    let first_in_pair = record.flags() & FIRST_IN_PAIR_FLAG != 0;
    ExternalBarcodeValues {
        primary: None,
        read_one: (!paired || first_in_pair)
            .then_some(config.read_one_barcode_tag.as_deref())
            .flatten()
            .and_then(|tag| bam_tag_value(record, tag)),
        read_two: (paired && !first_in_pair)
            .then_some(config.read_two_barcode_tag.as_deref())
            .flatten()
            .and_then(|tag| bam_tag_value(record, tag)),
    }
}

#[cfg(test)]
fn paired_external_barcode_values(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
) -> ExternalBarcodeValues {
    let primary = if first.flags & FIRST_IN_PAIR_FLAG != 0 {
        first.barcode.primary.clone()
    } else if second.flags & FIRST_IN_PAIR_FLAG != 0 {
        second.barcode.primary.clone()
    } else {
        first
            .barcode
            .primary
            .clone()
            .or_else(|| second.barcode.primary.clone())
    };
    ExternalBarcodeValues {
        primary,
        read_one: first
            .barcode
            .read_one
            .clone()
            .or_else(|| second.barcode.read_one.clone()),
        read_two: first
            .barcode
            .read_two
            .clone()
            .or_else(|| second.barcode.read_two.clone()),
    }
}

#[cfg(test)]
pub(crate) fn encode_external_plan_record(
    record: &ExternalPlanRecord,
    include_qname: bool,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(external_plan_record_encoded_len(record, include_qname));
    append_external_plan_record(&mut payload, record, include_qname);
    payload
}

#[cfg(test)]
fn external_barcode_encoded_len(value: Option<&[u8]>) -> usize {
    1 + value.map_or(0, |value| 8 + value.len())
}

#[cfg(test)]
fn external_plan_record_encoded_len(record: &ExternalPlanRecord, include_qname: bool) -> usize {
    let qname_bytes = if include_qname {
        4 + record.qname.len()
    } else {
        0
    };
    62 + qname_bytes
        + [
            record.barcode.primary.as_deref(),
            record.barcode.read_one.as_deref(),
            record.barcode.read_two.as_deref(),
            record.read_group.as_deref(),
        ]
        .into_iter()
        .map(external_barcode_encoded_len)
        .sum::<usize>()
}

pub(crate) fn append_external_plan_record(
    payload: &mut Vec<u8>,
    record: &ExternalPlanRecord,
    include_qname: bool,
) {
    payload.extend_from_slice(&record.ordinal.to_le_bytes());
    payload.extend_from_slice(&record.library_id.to_le_bytes());
    payload.extend_from_slice(&record.flags.to_le_bytes());
    payload.extend_from_slice(&record.reference_id.to_le_bytes());
    payload.extend_from_slice(&record.position.to_le_bytes());
    payload.extend_from_slice(&record.mate_reference_id.to_le_bytes());
    payload.extend_from_slice(&record.mate_position.to_le_bytes());
    payload.extend_from_slice(&record.template_length.to_le_bytes());
    payload.extend_from_slice(&record.unclipped_position.to_le_bytes());
    payload.extend_from_slice(&record.quality_score.to_le_bytes());
    if include_qname {
        payload.extend_from_slice(
            &u32::try_from(record.qname.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        payload.extend_from_slice(&record.qname);
    }
    encode_external_barcode_values(payload, &record.barcode);
    encode_external_barcode(payload, record.read_group.as_deref());
}

#[expect(
    clippy::too_many_arguments,
    reason = "shared encoder for fragment and record payloads without constructing ExternalPlanRecord"
)]
pub(crate) fn append_external_plan_parts(
    payload: &mut Vec<u8>,
    ordinal: u64,
    library_id: LibraryId,
    flags: u16,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    template_length: i64,
    unclipped_position: i64,
    quality_score: u64,
    qname: &[u8],
    barcode: &ExternalBarcodeValues,
    read_group: Option<&[u8]>,
) {
    payload.extend_from_slice(&ordinal.to_le_bytes());
    payload.extend_from_slice(&library_id.to_le_bytes());
    payload.extend_from_slice(&flags.to_le_bytes());
    payload.extend_from_slice(&reference_id.to_le_bytes());
    payload.extend_from_slice(&position.to_le_bytes());
    payload.extend_from_slice(&mate_reference_id.to_le_bytes());
    payload.extend_from_slice(&mate_position.to_le_bytes());
    payload.extend_from_slice(&template_length.to_le_bytes());
    payload.extend_from_slice(&unclipped_position.to_le_bytes());
    payload.extend_from_slice(&quality_score.to_le_bytes());
    payload.extend_from_slice(&u32::try_from(qname.len()).unwrap_or(u32::MAX).to_le_bytes());
    payload.extend_from_slice(qname);
    encode_external_barcode_values(payload, barcode);
    encode_external_barcode(payload, read_group);
}

#[cfg(test)]
pub(crate) fn encode_external_pair(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        8 + external_plan_record_encoded_len(first, true)
            + external_plan_record_encoded_len(second, true),
    );
    payload.extend_from_slice(&0_u64.to_le_bytes());
    append_external_plan_record(&mut payload, first, true);
    let first_length = (payload.len() - 8) as u64;
    payload[..8].copy_from_slice(&first_length.to_le_bytes());
    append_external_plan_record(&mut payload, second, true);
    payload
}

pub(crate) fn decode_borrowed_external_pair<'a>(
    key: &'a [u8],
    payload: &'a [u8],
) -> Result<[BorrowedExternalPlanRecord<'a>; 2], ExternalPlanError> {
    let mut offset = 0;
    let first_length = usize::try_from(read_external_u64(payload, &mut offset)?)
        .map_err(|_| ExternalPlanError::PairLengthTooLarge)?;
    let first = read_external_bytes(payload, &mut offset, first_length)?;
    let first = decode_borrowed_external_plan_record(key, first, true)?;
    let second = decode_borrowed_external_plan_record(key, &payload[offset..], true)?;
    if first.qname != second.qname || first.read_group != second.read_group {
        return Err(ExternalPlanError::PairIdentitiesDisagree);
    }
    Ok([first, second])
}

pub(crate) fn decode_external_plan_record(
    key: &[u8],
    payload: &[u8],
    qname_in_payload: bool,
) -> Result<ExternalPlanRecord, ExternalPlanError> {
    decode_borrowed_external_plan_record(key, payload, qname_in_payload)
        .map(BorrowedExternalPlanRecord::into_owned)
}

pub(crate) fn decode_borrowed_external_plan_record<'a>(
    key: &'a [u8],
    payload: &'a [u8],
    qname_in_payload: bool,
) -> Result<BorrowedExternalPlanRecord<'a>, ExternalPlanError> {
    let mut offset = 0usize;
    let ordinal = read_external_u64(payload, &mut offset)?;
    let library_id = read_external_u32(payload, &mut offset)?;
    let flags = read_external_u16(payload, &mut offset)?;
    let reference_id = read_external_i32(payload, &mut offset)?;
    let position = read_external_i64(payload, &mut offset)?;
    let mate_reference_id = read_external_i32(payload, &mut offset)?;
    let mate_position = read_external_i64(payload, &mut offset)?;
    let template_length = read_external_i64(payload, &mut offset)?;
    let unclipped_position = read_external_i64(payload, &mut offset)?;
    let quality_score = read_external_u64(payload, &mut offset)?;
    let qname = if qname_in_payload {
        let length = usize::try_from(read_external_u32(payload, &mut offset)?)
            .map_err(|_| ExternalPlanError::QnameLengthTooLarge)?;
        read_external_bytes(payload, &mut offset, length)?
    } else {
        bounded_plan::template_qname(key)?
    };
    let barcode = [
        decode_borrowed_external_barcode(payload, &mut offset)?,
        decode_borrowed_external_barcode(payload, &mut offset)?,
        decode_borrowed_external_barcode(payload, &mut offset)?,
    ];
    let read_group = decode_borrowed_external_barcode(payload, &mut offset)?;
    if offset != payload.len() {
        return Err(ExternalPlanError::TrailingPayloadBytes);
    }
    if !qname_in_payload && bounded_plan::template_key(read_group, qname) != key {
        return Err(ExternalPlanError::TemplateKeyDisagrees);
    }
    Ok(BorrowedExternalPlanRecord {
        ordinal,
        library_id,
        read_group,
        flags,
        reference_id,
        position,
        mate_reference_id,
        mate_position,
        template_length,
        unclipped_position,
        quality_score,
        qname,
        barcode,
    })
}

pub(crate) fn decode_external_barcode_values_from(
    payload: &[u8],
    offset: &mut usize,
) -> Result<ExternalBarcodeValues, ExternalPlanError> {
    Ok(ExternalBarcodeValues {
        primary: decode_borrowed_external_barcode(payload, offset)?.map(<[u8]>::to_vec),
        read_one: decode_borrowed_external_barcode(payload, offset)?.map(<[u8]>::to_vec),
        read_two: decode_borrowed_external_barcode(payload, offset)?.map(<[u8]>::to_vec),
    })
}

pub(crate) fn compact_pair_key_into(
    key: &mut Vec<u8>,
    first: &PendingMate,
    second: &PendingMate,
    barcodes_enabled: bool,
) -> bool {
    let first_is_left = (first.reference_id, first.unclipped_position)
        <= (second.reference_id, second.unclipped_position);
    let (left, right) = if first_is_left {
        (first, second)
    } else {
        (second, first)
    };
    let left_reverse = u8::from(left.flags & 0x10 != 0);
    let right_reverse = u8::from(right.flags & 0x10 != 0);
    key.clear();
    key.extend_from_slice(&first.library_id.to_be_bytes());
    key.extend_from_slice(&(left.reference_id.cast_unsigned() ^ (1_u32 << 31)).to_be_bytes());
    key.extend_from_slice(&(left.unclipped_position.cast_unsigned() ^ (1_u64 << 63)).to_be_bytes());
    key.extend_from_slice(&(right.reference_id.cast_unsigned() ^ (1_u32 << 31)).to_be_bytes());
    key.extend_from_slice(
        &(right.unclipped_position.cast_unsigned() ^ (1_u64 << 63)).to_be_bytes(),
    );
    key.push((left_reverse << 1) | right_reverse);
    key.push(0);
    if barcodes_enabled {
        let primary = if first.flags & FIRST_IN_PAIR_FLAG != 0 {
            first.barcode.primary.as_deref()
        } else if second.flags & FIRST_IN_PAIR_FLAG != 0 {
            second.barcode.primary.as_deref()
        } else {
            first
                .barcode
                .primary
                .as_deref()
                .or(second.barcode.primary.as_deref())
        };
        let read_one = first
            .barcode
            .read_one
            .as_deref()
            .or(second.barcode.read_one.as_deref());
        let read_two = first
            .barcode
            .read_two
            .as_deref()
            .or(second.barcode.read_two.as_deref());
        encode_external_barcode(key, primary);
        encode_external_barcode(key, read_one);
        encode_external_barcode(key, read_two);
    } else {
        key.extend_from_slice(&[0, 0, 0]);
    }
    first_is_left
}

pub(crate) fn decode_compact_pair_key_coords(
    key: &[u8],
) -> Result<(i32, i64, i32, i64), ExternalPlanError> {
    if key.len() < 28 {
        return Err(ExternalPlanError::TruncatedPayload);
    }
    let left_ref =
        (u32::from_be_bytes([key[4], key[5], key[6], key[7]]) ^ (1_u32 << 31)).cast_signed();
    let left_pos = (u64::from_be_bytes([
        key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15],
    ]) ^ (1_u64 << 63))
        .cast_signed();
    let right_ref =
        (u32::from_be_bytes([key[16], key[17], key[18], key[19]]) ^ (1_u32 << 31)).cast_signed();
    let right_pos = (u64::from_be_bytes([
        key[20], key[21], key[22], key[23], key[24], key[25], key[26], key[27],
    ]) ^ (1_u64 << 63))
        .cast_signed();
    Ok((left_ref, left_pos, right_ref, right_pos))
}

pub(crate) fn encode_pending_mate_into(
    payload: &mut Vec<u8>,
    mate: &PendingMate,
    barcodes_enabled: bool,
) {
    payload.clear();
    payload.extend_from_slice(&mate.ordinal.to_le_bytes());
    payload.extend_from_slice(&mate.library_id.to_le_bytes());
    payload.extend_from_slice(&mate.rg_id.to_le_bytes());
    payload.extend_from_slice(&mate.flags.to_le_bytes());
    payload.extend_from_slice(&mate.reference_id.to_le_bytes());
    payload.extend_from_slice(&mate.position.to_le_bytes());
    payload.extend_from_slice(&mate.mate_reference_id.to_le_bytes());
    payload.extend_from_slice(&mate.unclipped_position.to_le_bytes());
    payload.extend_from_slice(&mate.quality_score.to_le_bytes());
    if barcodes_enabled {
        encode_external_barcode_values(payload, &mate.barcode);
    }
}

pub(crate) fn decode_pending_mate_into(
    payload: &[u8],
    barcodes_enabled: bool,
    out: &mut PendingMate,
) -> Result<(), ExternalPlanError> {
    let mut offset = 0usize;
    out.ordinal = read_external_u64(payload, &mut offset)?;
    out.library_id = read_external_u32(payload, &mut offset)?;
    out.rg_id = read_external_u32(payload, &mut offset)?;
    out.flags = read_external_u16(payload, &mut offset)?;
    out.reference_id = read_external_i32(payload, &mut offset)?;
    out.position = read_external_i64(payload, &mut offset)?;
    out.mate_reference_id = read_external_i32(payload, &mut offset)?;
    out.unclipped_position = read_external_i64(payload, &mut offset)?;
    out.quality_score = read_external_u64(payload, &mut offset)?;
    if barcodes_enabled {
        out.barcode.primary =
            decode_borrowed_external_barcode(payload, &mut offset)?.map(<[u8]>::to_vec);
        out.barcode.read_one =
            decode_borrowed_external_barcode(payload, &mut offset)?.map(<[u8]>::to_vec);
        out.barcode.read_two =
            decode_borrowed_external_barcode(payload, &mut offset)?.map(<[u8]>::to_vec);
    } else {
        out.barcode.primary = None;
        out.barcode.read_one = None;
        out.barcode.read_two = None;
    }
    if offset != payload.len() {
        return Err(ExternalPlanError::TrailingPayloadBytes);
    }
    Ok(())
}

pub(crate) fn append_compact_pair(
    payload: &mut Vec<u8>,
    first: &PendingMate,
    second: &PendingMate,
    qname: &[u8],
    first_is_left: bool,
    barcodes_enabled: bool,
) {
    payload.clear();
    payload.extend_from_slice(&first.ordinal.to_le_bytes());
    payload.extend_from_slice(&second.ordinal.to_le_bytes());
    payload.extend_from_slice(&first.flags.to_le_bytes());
    payload.extend_from_slice(&second.flags.to_le_bytes());
    payload.extend_from_slice(&first.library_id.to_le_bytes());
    payload.extend_from_slice(&first.rg_id.to_le_bytes());
    payload.extend_from_slice(
        &first
            .quality_score
            .saturating_add(second.quality_score)
            .to_le_bytes(),
    );
    payload.push(u8::from(first_is_left));
    payload.extend_from_slice(&u32::try_from(qname.len()).unwrap_or(u32::MAX).to_le_bytes());
    payload.extend_from_slice(qname);
    if barcodes_enabled {
        encode_external_barcode_values(payload, &first.barcode);
        encode_external_barcode_values(payload, &second.barcode);
    }
}

pub(crate) fn decode_compact_pair_header(
    payload: &[u8],
    barcodes_enabled: bool,
) -> Result<CompactPairHeader<'_>, ExternalPlanError> {
    let mut offset = 0usize;
    let first_ordinal = read_external_u64(payload, &mut offset)?;
    let second_ordinal = read_external_u64(payload, &mut offset)?;
    let first_flags = read_external_u16(payload, &mut offset)?;
    let second_flags = read_external_u16(payload, &mut offset)?;
    let library_id = read_external_u32(payload, &mut offset)?;
    let rg_id = read_external_u32(payload, &mut offset)?;
    let combined_score = read_external_u64(payload, &mut offset)?;
    let first_is_left = match read_external_u8(payload, &mut offset)? {
        0 => false,
        1 => true,
        _ => return Err(ExternalPlanError::InvalidTemplateMarker),
    };
    let qname_len = usize::try_from(read_external_u32(payload, &mut offset)?)
        .map_err(|_| ExternalPlanError::QnameLengthTooLarge)?;
    let qname = read_external_bytes(payload, &mut offset, qname_len)?;
    let barcodes_tail = &payload[offset..];
    if !barcodes_enabled && !barcodes_tail.is_empty() {
        return Err(ExternalPlanError::TrailingPayloadBytes);
    }
    Ok(CompactPairHeader {
        first_ordinal,
        second_ordinal,
        first_flags,
        second_flags,
        library_id,
        rg_id,
        combined_score,
        first_is_left,
        qname,
        barcodes_tail,
    })
}

pub(crate) fn external_fragment_key_into(
    key: &mut Vec<u8>,
    library_id: LibraryId,
    reference_id: i32,
    unclipped_position: i64,
    reverse_strand: bool,
    barcode: &ExternalBarcodeValues,
) {
    external_key_into(
        key,
        &ExternalKey {
            library_id,
            reference_id,
            position: unclipped_position,
            mate_reference_id: -1,
            mate_position: -1,
            orientation: 0,
            reverse_strand,
            barcode,
        },
    );
}

#[cfg(test)]
pub(crate) fn external_pair_key(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
) -> Vec<u8> {
    let (left, right) = if (first.reference_id, first.unclipped_position)
        <= (second.reference_id, second.unclipped_position)
    {
        (first, second)
    } else {
        (second, first)
    };
    let left_reverse = u8::from(left.flags & 0x10 != 0);
    let right_reverse = u8::from(right.flags & 0x10 != 0);
    let barcode = paired_external_barcode_values(first, second);
    external_key(&ExternalKey {
        library_id: first.library_id,
        reference_id: left.reference_id,
        position: left.unclipped_position,
        mate_reference_id: right.reference_id,
        mate_position: right.unclipped_position,
        orientation: (left_reverse << 1) | right_reverse,
        reverse_strand: false,
        barcode: &barcode,
    })
}

#[cfg(test)]
pub(crate) fn external_fragment_key(record: &ExternalPlanRecord) -> Vec<u8> {
    external_key(&ExternalKey {
        library_id: record.library_id,
        reference_id: record.reference_id,
        position: record.unclipped_position,
        mate_reference_id: -1,
        mate_position: -1,
        orientation: 0,
        reverse_strand: record.flags & 0x10 != 0,
        barcode: &record.barcode,
    })
}

pub(crate) fn encode_external_decision_payload(decision: &ExternalDecision) -> Vec<u8> {
    let mut payload = Vec::with_capacity(EXTERNAL_DECISION_PAYLOAD_BYTES);
    payload.push(decision.flags);
    payload.extend_from_slice(
        &decision
            .duplicate_set_size
            .unwrap_or(EXTERNAL_DECISION_ABSENT_TAG)
            .to_le_bytes(),
    );
    payload.extend_from_slice(
        &decision
            .duplicate_set_index
            .unwrap_or(EXTERNAL_DECISION_ABSENT_TAG)
            .to_le_bytes(),
    );
    payload
}

pub(crate) fn decode_external_decision_payload(
    payload: &[u8],
) -> Result<ExternalDecision, ExternalPlanError> {
    let &[
        flags,
        size0,
        size1,
        size2,
        size3,
        index0,
        index1,
        index2,
        index3,
    ] = payload
    else {
        return Err(ExternalPlanError::InvalidDecisionPayloadLength);
    };
    let known_flags =
        EXTERNAL_DECISION_OPTICAL | EXTERNAL_DECISION_DUPLICATE | EXTERNAL_DECISION_SET_MEMBERS;
    if flags & !known_flags != 0 {
        return Err(ExternalPlanError::InvalidDecisionFlags);
    }
    let size = i32::from_le_bytes([size0, size1, size2, size3]);
    let index = i32::from_le_bytes([index0, index1, index2, index3]);
    let has_set_members = flags & EXTERNAL_DECISION_SET_MEMBERS != 0;
    if has_set_members
        == (size == EXTERNAL_DECISION_ABSENT_TAG || index == EXTERNAL_DECISION_ABSENT_TAG)
    {
        return Err(ExternalPlanError::IncompleteDuplicateSetMetadata);
    }
    Ok(ExternalDecision {
        flags,
        duplicate_set_size: (size != EXTERNAL_DECISION_ABSENT_TAG).then_some(size),
        duplicate_set_index: (index != EXTERNAL_DECISION_ABSENT_TAG).then_some(index),
    })
}

fn external_key_into(key: &mut Vec<u8>, key_fields: &ExternalKey<'_>) {
    key.clear();
    key.extend_from_slice(&key_fields.library_id.to_be_bytes());
    key.extend_from_slice(&(key_fields.reference_id.cast_unsigned() ^ (1_u32 << 31)).to_be_bytes());
    key.extend_from_slice(&(key_fields.position.cast_unsigned() ^ (1_u64 << 63)).to_be_bytes());
    key.extend_from_slice(
        &(key_fields.mate_reference_id.cast_unsigned() ^ (1_u32 << 31)).to_be_bytes(),
    );
    key.extend_from_slice(
        &(key_fields.mate_position.cast_unsigned() ^ (1_u64 << 63)).to_be_bytes(),
    );
    key.push(key_fields.orientation);
    key.push(u8::from(key_fields.reverse_strand));
    encode_external_barcode(key, key_fields.barcode.primary.as_deref());
    encode_external_barcode(key, key_fields.barcode.read_one.as_deref());
    encode_external_barcode(key, key_fields.barcode.read_two.as_deref());
}

#[cfg(test)]
fn external_key(key_fields: &ExternalKey<'_>) -> Vec<u8> {
    let barcode_bytes = [
        key_fields.barcode.primary.as_deref(),
        key_fields.barcode.read_one.as_deref(),
        key_fields.barcode.read_two.as_deref(),
    ]
    .into_iter()
    .map(external_barcode_encoded_len)
    .sum::<usize>();
    let mut key = Vec::with_capacity(4 + 4 + 8 + 4 + 8 + 2 + barcode_bytes);
    external_key_into(&mut key, key_fields);
    key
}

fn encode_external_barcode_values(payload: &mut Vec<u8>, barcode: &ExternalBarcodeValues) {
    encode_external_barcode(payload, barcode.primary.as_deref());
    encode_external_barcode(payload, barcode.read_one.as_deref());
    encode_external_barcode(payload, barcode.read_two.as_deref());
}

fn encode_external_barcode(payload: &mut Vec<u8>, barcode: Option<&[u8]>) {
    match barcode {
        None => payload.push(0),
        Some(barcode) => {
            payload.push(1);
            payload.extend_from_slice(
                &u64::try_from(barcode.len())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            payload.extend_from_slice(barcode);
        }
    }
}

fn decode_borrowed_external_barcode<'a>(
    payload: &'a [u8],
    offset: &mut usize,
) -> Result<Option<&'a [u8]>, ExternalPlanError> {
    match read_external_u8(payload, offset)? {
        0 => Ok(None),
        1 => {
            let length = usize::try_from(read_external_u64(payload, offset)?)
                .map_err(|_| ExternalPlanError::BarcodeLengthTooLarge)?;
            Ok(Some(read_external_bytes(payload, offset, length)?))
        }
        _ => Err(ExternalPlanError::InvalidBarcodeMarker),
    }
}

fn read_external_bytes<'a>(
    payload: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ExternalPlanError> {
    let end = offset
        .checked_add(length)
        .ok_or(ExternalPlanError::PayloadLengthOverflow)?;
    let bytes = payload
        .get(*offset..end)
        .ok_or(ExternalPlanError::TruncatedPayload)?;
    *offset = end;
    Ok(bytes)
}

fn read_external_u8(payload: &[u8], offset: &mut usize) -> Result<u8, ExternalPlanError> {
    let &[byte] = read_external_bytes(payload, offset, 1)? else {
        return Err(ExternalPlanError::TruncatedPayload);
    };
    Ok(byte)
}

fn read_external_u16(payload: &[u8], offset: &mut usize) -> Result<u16, ExternalPlanError> {
    let mut bytes = [0_u8; 2];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u16::from_le_bytes(bytes))
}

fn read_external_u32(payload: &[u8], offset: &mut usize) -> Result<u32, ExternalPlanError> {
    let mut bytes = [0_u8; 4];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u32::from_le_bytes(bytes))
}

fn read_external_u64(payload: &[u8], offset: &mut usize) -> Result<u64, ExternalPlanError> {
    let mut bytes = [0_u8; 8];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u64::from_le_bytes(bytes))
}

fn read_external_i32(payload: &[u8], offset: &mut usize) -> Result<i32, ExternalPlanError> {
    let mut bytes = [0_u8; 4];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(i32::from_le_bytes(bytes))
}

fn read_external_i64(payload: &[u8], offset: &mut usize) -> Result<i64, ExternalPlanError> {
    let mut bytes = [0_u8; 8];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(i64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mate(ordinal: u64, reference_id: i32, unclipped_position: i64) -> PendingMate {
        PendingMate {
            ordinal,
            library_id: 7,
            rg_id: 2,
            flags: 0x41,
            reference_id,
            position: unclipped_position,
            mate_reference_id: reference_id,
            unclipped_position,
            quality_score: 1200,
            barcode: ExternalBarcodeValues {
                primary: Some(b"ACGT".to_vec()),
                read_one: Some(b"TT".to_vec()),
                read_two: None,
            },
        }
    }

    #[test]
    fn compact_pair_key_round_trips_signed_coordinates() {
        let first = sample_mate(1, 2, 500);
        let second = sample_mate(2, -1, -42);
        let mut key = Vec::new();
        let first_is_left = compact_pair_key_into(&mut key, &first, &second, false);
        assert!(!first_is_left);
        assert_eq!(
            decode_compact_pair_key_coords(&key).unwrap(),
            (-1, -42, 2, 500)
        );
        assert!(matches!(
            decode_compact_pair_key_coords(&key[..10]),
            Err(ExternalPlanError::TruncatedPayload)
        ));
    }

    #[test]
    fn pending_mate_round_trips_with_and_without_barcodes() {
        let mate = sample_mate(9, 1, 250);
        let mut buf = Vec::new();
        let mut decoded = PendingMate::empty();

        encode_pending_mate_into(&mut buf, &mate, true);
        decode_pending_mate_into(&buf, true, &mut decoded).unwrap();
        assert_eq!(decoded.ordinal, mate.ordinal);
        assert_eq!(decoded.barcode, mate.barcode);

        encode_pending_mate_into(&mut buf, &mate, false);
        decode_pending_mate_into(&buf, false, &mut decoded).unwrap();
        assert_eq!(decoded.barcode, ExternalBarcodeValues::empty());
    }

    #[test]
    fn external_decision_payload_round_trips() {
        let decision = ExternalDecision {
            flags: EXTERNAL_DECISION_DUPLICATE | EXTERNAL_DECISION_SET_MEMBERS,
            duplicate_set_size: Some(4),
            duplicate_set_index: Some(2),
        };
        let encoded = encode_external_decision_payload(&decision);
        assert_eq!(
            decode_external_decision_payload(&encoded).unwrap(),
            decision
        );
    }
}
