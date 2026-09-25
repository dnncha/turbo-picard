//! Small, independently tested contracts used by the external duplicate plan.

/// Coordinate sorting does not impose a read-name or flag order at equal
/// positions. Unplaced records (RNAME=*) belong after every placed record.
pub(super) fn coordinate_key(tid: i32, position: i64) -> (i32, i64) {
    if tid < 0 {
        (i32::MAX, i64::MAX)
    } else {
        (tid, position)
    }
}

/// A template belongs to a read group as well as a read name. Length framing
/// prevents ambiguities such as ("a", "bc") versus ("ab", "c"), and keeps an
/// absent read group distinct from an empty value. The suffix remains the
/// original QNAME, so no duplicate copy is needed in the QNAME-sort payload.
pub(super) fn template_key(read_group: Option<&[u8]>, qname: &[u8]) -> Vec<u8> {
    let group = read_group.unwrap_or_default();
    let mut key = Vec::with_capacity(9 + group.len() + qname.len());
    key.push(u8::from(read_group.is_some()));
    key.extend_from_slice(&(group.len() as u64).to_le_bytes());
    key.extend_from_slice(group);
    key.extend_from_slice(qname);
    key
}

pub(super) fn template_qname(key: &[u8]) -> Result<&[u8], String> {
    let prefix = key
        .get(..9)
        .ok_or_else(|| "external MarkDuplicates template key is truncated".to_string())?;
    if prefix[0] > 1 {
        return Err("external MarkDuplicates template marker is invalid".to_string());
    }
    let mut length = [0_u8; 8];
    length.copy_from_slice(&prefix[1..]);
    let length = usize::try_from(u64::from_le_bytes(length))
        .map_err(|_| "external MarkDuplicates read-group key is too large".to_string())?;
    if prefix[0] == 0 && length != 0 {
        return Err("absent read group has a nonempty template key".to_string());
    }
    let end = 9_usize
        .checked_add(length)
        .ok_or_else(|| "external MarkDuplicates template length overflow".to_string())?;
    key.get(end..)
        .ok_or_else(|| "external MarkDuplicates read-group key is truncated".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_ties_ignore_names_and_unplaced_reads_sort_last() {
        assert_eq!(coordinate_key(0, 7), (0, 7));
        assert!(coordinate_key(0, 7) < coordinate_key(1, 0));
        assert!(coordinate_key(100, 900) < coordinate_key(-1, -1));
        assert_eq!(coordinate_key(-1, -1), coordinate_key(-1, 0));
    }

    #[test]
    fn template_identity_is_framed_and_lossless() {
        let combinations = [
            (None, b"name".as_slice()),
            (Some(b"".as_slice()), b"name".as_slice()),
            (Some(b"a".as_slice()), b"bc".as_slice()),
            (Some(b"ab".as_slice()), b"c".as_slice()),
            (Some(b"rg1".as_slice()), b"shared".as_slice()),
            (Some(b"rg2".as_slice()), b"shared".as_slice()),
        ];
        let keys: Vec<_> = combinations
            .iter()
            .map(|(group, name)| template_key(*group, name))
            .collect();
        for (index, key) in keys.iter().enumerate() {
            assert_eq!(template_qname(key).unwrap(), combinations[index].1);
            for other in &keys[index + 1..] {
                assert_ne!(key, other);
            }
        }
    }

    #[test]
    fn malformed_template_keys_fail_closed() {
        let key = template_key(Some(b"rg"), b"read");
        for end in 0..11 {
            assert!(template_qname(&key[..end]).is_err());
        }
        let mut invalid = key.clone();
        invalid[0] = 2;
        assert!(template_qname(&invalid).is_err());
        invalid[0] = 0;
        assert!(template_qname(&invalid).is_err());
        invalid[0] = 1;
        invalid[1..9].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(template_qname(&invalid).is_err());
    }
}
