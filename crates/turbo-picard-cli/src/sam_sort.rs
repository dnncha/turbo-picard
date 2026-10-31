use super::{SortOrder, aux_i32, bam};

pub(super) struct Fields<'a> {
    pub name: &'a [u8],
    pub flags: u16,
    pub tid: i32,
    pub position: i64,
    pub mapping_quality: u8,
    pub mate_tid: i32,
    pub mate_position: i64,
    pub template_length: i64,
    pub hit_index: Option<i32>,
}

impl<'a> Fields<'a> {
    pub fn from_bam(record: &'a bam::Record, include_hit_index: bool) -> Self {
        Self {
            name: record.qname(),
            flags: record.flags(),
            tid: record.tid(),
            position: record.pos(),
            mapping_quality: record.mapq(),
            mate_tid: record.mtid(),
            mate_position: record.mpos(),
            template_length: record.insert_size(),
            hit_index: if include_hit_index {
                record.aux(b"HI").ok().and_then(aux_i32)
            } else {
                None
            },
        }
    }

    pub fn coordinate(&self) -> (i32, i64, bool, &[u8], u16, u8, i32, i64, i64) {
        let (tid, position) = if self.tid < 0 {
            (i32::MAX, 0)
        } else {
            (self.tid, self.position)
        };
        (
            tid,
            position,
            self.flags & 0x10 != 0,
            self.name,
            self.flags,
            self.mapping_quality,
            self.mate_tid,
            self.mate_position,
            self.template_length,
        )
    }

    pub fn queryname(&self) -> (&[u8], bool, bool, bool, bool, bool, Option<i32>) {
        let paired = self.flags & 1 != 0;
        (
            self.name,
            !paired,
            paired && self.flags & 0x80 != 0,
            self.flags & 0x10 != 0,
            self.flags & 0x100 != 0,
            self.flags & 0x800 != 0,
            self.hit_index,
        )
    }

    pub fn bytes(&self, order: SortOrder) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.name.len() + 48);
        match order {
            SortOrder::Coordinate => {
                let (tid, position, reverse, name, flags, mapq, mate_tid, mate_position, length) =
                    self.coordinate();
                bytes.extend((tid.cast_unsigned() ^ (1 << 31)).to_be_bytes());
                bytes.extend((position.cast_unsigned() ^ (1 << 63)).to_be_bytes());
                bytes.push(u8::from(reverse));
                bytes.extend(name);
                bytes.push(0);
                bytes.extend(flags.to_be_bytes());
                bytes.push(mapq);
                bytes.extend((mate_tid.cast_unsigned() ^ (1 << 31)).to_be_bytes());
                bytes.extend((mate_position.cast_unsigned() ^ (1 << 63)).to_be_bytes());
                bytes.extend((length.cast_unsigned() ^ (1 << 63)).to_be_bytes());
            }
            SortOrder::QueryName => {
                let (name, unpaired, second, reverse, secondary, supplementary, hit) =
                    self.queryname();
                bytes.extend(name);
                bytes.push(0);
                bytes.extend([unpaired, second, reverse, secondary, supplementary].map(u8::from));
                bytes.push(u8::from(hit.is_some()));
                if let Some(hit) = hit {
                    bytes.extend((hit.cast_unsigned() ^ (1 << 31)).to_be_bytes());
                }
            }
            SortOrder::Unsorted => {}
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Aux;
    #[test]
    fn spill_keys_and_decoded_comparison_share_order_including_ties() {
        let mut records = Vec::new();
        for (index, flags) in (0_i32..).zip([0, 16, 99, 147, 355, 2147, 2209]) {
            let mut record = bam::Record::new();
            record.set(if index % 2 == 0 { b"a" } else { b"b" }, None, &[], &[]);
            record.set_flags(flags);
            record.set_tid(if index % 3 == 0 { -1 } else { 0 });
            record.set_pos(i64::from(index) - 3);
            record.set_mtid(-1);
            record.set_mpos(-1);
            record.set_insert_size(i64::from(index) - 4);
            if index % 2 == 0 {
                record.push_aux(b"HI", Aux::I32(index - 3)).unwrap();
            }
            records.push(record);
        }
        for left in &records {
            for right in &records {
                let left = Fields::from_bam(left, true);
                let right = Fields::from_bam(right, true);
                assert_eq!(
                    left.bytes(SortOrder::Coordinate)
                        .cmp(&right.bytes(SortOrder::Coordinate)),
                    left.coordinate().cmp(&right.coordinate())
                );
                assert_eq!(
                    left.bytes(SortOrder::QueryName)
                        .cmp(&right.bytes(SortOrder::QueryName)),
                    left.queryname().cmp(&right.queryname())
                );
            }
        }
    }
}
