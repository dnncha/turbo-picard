"""One-shot, hash-pinned source migration; remove after the repair commit."""
from pathlib import Path
import subprocess

path = Path('crates/turbo-picard-markdup/src/lib.rs')
expected = '1c1a5d84b8f581825f56284ff5ddd5aca9f2cec0'
actual = subprocess.check_output(['git', 'hash-object', str(path)], text=True).strip()
if actual != expected:
    raise SystemExit(f'Refusing changed source: {actual} != {expected}')
text = path.read_text()

def once(source, old, new):
    count = source.count(old)
    if count != 1:
        raise ValueError(f'Expected one occurrence, found {count}: {old[:120]!r}')
    return source.replace(old, new, 1)

def function(name, transform):
    global text
    start = text.index('\nfn ' + name + '(')
    end = text.find('\nfn ', start + 1)
    if end < 0:
        end = len(text)
    text = text[:start] + transform(text[start:end]) + text[end:]

text = once(text, '#![forbid(unsafe_code)]\n', '#![forbid(unsafe_code)]\n\nmod bounded_plan;\n#[cfg(test)]\nmod bounded_plan_tests;\n')
text = once(text, 'use std::io::{BufWriter, Read as IoRead, Write};', 'use std::io::{BufReader, BufWriter, Read as IoRead, Write};')
text = once(text, 'struct ExternalPlanRecord {\n    ordinal: u64,\n    library_id: LibraryId,\n    read_group: Option<Vec<u8>>,', 'struct ExternalPlanRecord {\n    ordinal: u64,\n    library_id: LibraryId,\n    read_group: Option<Vec<u8>>,\n    template_key: Vec<u8>,')

function('external_plan_record', lambda s: once(s,
    '    ExternalPlanRecord {\n        ordinal,\n        library_id,\n        read_group: record_read_group(record),',
    '    let read_group = record_read_group(record);\n    let template_key = bounded_plan::template_key(read_group.as_deref(), record.qname());\n    ExternalPlanRecord {\n        ordinal,\n        library_id,\n        read_group,\n        template_key,'))

def external(s):
    s = once(s, '    let mut summary = MarkDuplicatesSummary {', '    // The header-only reader must not retain an idle HTS worker pool.\n    drop(reader);\n    let mut summary = MarkDuplicatesSummary {')
    s = once(s, '    let mut last_output_order = None::<(i32, i64, Vec<u8>, u16)>;', '    let mut last_output_order = None::<(i32, i64, Vec<u8>, u16)>;\n    let mut last_coordinate = None::<(i32, i64)>;')
    s = once(s, '        for result in reader.records() {\n            let mut record = result?;', '        let mut record = bam::Record::new();\n        while let Some(result) = reader.read(&mut record) {\n            result?;')
    old = '''            let output_order = (record.tid(), record.pos(), record.qname().to_vec(), flag);
            if last_output_order
                .as_ref()
                .is_some_and(|previous| output_order.cmp(previous) == Ordering::Less)
            {
                // The compact multi-input path sorts the final records by
                // coordinate.  Do not silently change that contract here:
                // fall back if the input streams are not already globally
                // ordered, so the bounded path remains deterministic.
                return Ok(None);
            }
            last_output_order = Some(output_order);'''
    new = '''            if config.inputs.len() == 1 {
                // A single input is replayed unchanged. Coordinate order permits
                // arbitrary QNAME/flag ties and puts unplaced records last.
                let coordinate = bounded_plan::coordinate_key(record.tid(), record.pos());
                if last_coordinate.is_some_and(|previous| coordinate < previous) {
                    return Ok(None);
                }
                last_coordinate = Some(coordinate);
            } else {
                // Preserve the existing multi-input output-order contract. This
                // is deliberately separate from single-input eligibility.
                let output_order = (record.tid(), record.pos(), record.qname().to_vec(), flag);
                if last_output_order
                    .as_ref()
                    .is_some_and(|previous| output_order.cmp(previous) == Ordering::Less)
                {
                    return Ok(None);
                }
                last_output_order = Some(output_order);
            }'''
    s = once(s, old, new)
    s = once(s, '                        plan.qname.clone(),', '                        plan.template_key.clone(),')
    s = once(s, '    let mut pair_sorter = external_sorter', '    if !config.quiet {\n        eprintln!("MarkDuplicates: using external-sort plan ({record_count} records)");\n    }\n    let mut pair_sorter = external_sorter')
    s = once(s, 'pending.qname.as_slice() != item.key.as_slice()', 'pending.template_key.as_slice() != item.key.as_slice()')
    s = once(s, '                if first.qname == plan.qname {', '''                if first.template_key == plan.template_key {
                    // Do not silently fabricate a template from two first (or
                    // two second) primary alignments with the same identity.
                    if !matches!((first.flags & 0xc0, plan.flags & 0xc0), (0x40, 0x80) | (0x80, 0x40)) {
                        return Err("external MarkDuplicates mates must have complementary first/second flags".to_string());
                    }''')
    s = once(s, '    let mut decisions = File::open(&decision_path)?;', '    let mut decisions = BufReader::new(File::open(&decision_path)?);')
    return s
function('try_run_external_plan', external)

def decode(s):
    s = once(s, '        key.to_vec()\n', '        bounded_plan::template_qname(key)?.to_vec()\n')
    s = once(s, '    Ok(ExternalPlanRecord {', '''    let template_key = bounded_plan::template_key(read_group.as_deref(), &qname);
    if !qname_in_payload && template_key != key {
        return Err("external MarkDuplicates template key disagrees with its payload".to_string());
    }
    Ok(ExternalPlanRecord {''')
    s = once(s, '        read_group,\n', '        read_group,\n        template_key,\n')
    return s
function('decode_external_plan_record', decode)

def groups(s):
    s = once(s, '.entry(member.qname.clone())', '.entry(member.template_key.clone())')
    start = s.index('    let mut reads = names\n')
    end = s.index('    reads.sort_by_key', start)
    s = s[:start] + '''    // Visit the family once instead of searching the entire family for each
    // distinct template (quadratic on large PCR duplicate families).
    let mut reads = group
        .iter()
        .filter(|member| names.get(member.template_key.as_slice()).is_some_and(|(_, ordinal)| *ordinal == member.ordinal))
        .map(|member| OpticalRead {
            name: member.template_key.clone(),
            location: processing_config
                .read_name_parser
                .coordinates(member.qname.as_slice())
                .map(|(tile, x, y)| ReadLocation {
                    read_group: member.read_group.clone(),
                    tile,
                    x,
                    y,
                }),
        })
        .collect::<Vec<_>>();
''' + s[end:]
    s = s.replace('member.qname.as_slice() == representative_name', 'member.template_key.as_slice() == representative_name')
    s = once(s, 'optical_names.contains(member.qname.as_slice())', 'optical_names.contains(member.template_key.as_slice())')
    return s
function('process_external_duplicate_group', groups)
function('process_external_fragment_group', lambda s: once(s, 'member.qname.as_slice() != representative_name', 'member.template_key.as_slice() != representative_name'))
function('read_external_duplicate_decision', lambda s: once(s, '    reader: &mut File,', '    reader: &mut impl IoRead,'))
function('write_external_plan_records', lambda s: once(s, '    decisions: &mut File,', '    decisions: &mut impl IoRead,'))

# Fast paths only have single-read-group identity semantics. Use the corrected
# external engine for multiple groups even when an input happens to be small.
function('try_run_small_single_bam_compact_plan', lambda s: once(s,
    '    let mut reader = open_markdup_reader(config, &config.inputs[0])?;',
    '    let mut reader = open_markdup_reader(config, &config.inputs[0])?;\n    if read_group_ids(reader.header()).len() > 1 {\n        return Ok(None);\n    }'))

def fast(s):
    s = once(s, '    let mut reader = open_markdup_reader(config, &config.input)?;', '    let mut reader = open_markdup_reader(config, &config.input)?;\n    if read_group_ids(reader.header()).len() > 1 {\n        return Ok(None);\n    }')
    s = once(s, '    for result in reader.records() {\n        let record = result?;', '''    for (record_index, result) in reader.records().enumerate() {
        let record = result?;
        // A speculative no-duplicate probe must not grow whole-file hash tables.
        // Its temporary output is discarded by the existing fallback cleanup.
        if record_index >= COMPACT_MARKDUP_MAX_RECORDS {
            should_fallback = true;
            break;
        }''')
    return s
function('try_run_single_bam_no_duplicate_fast_path', fast)
path.write_text(text)
print('Applied bounded MarkDuplicates repair to the pinned source.')
