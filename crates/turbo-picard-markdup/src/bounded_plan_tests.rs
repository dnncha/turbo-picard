use super::*;
use rust_htslib::bam::record::{Cigar, CigarString};
use std::io::{self, BufReader, Cursor};

fn config(dir: &Path) -> MarkDuplicatesConfig {
    let input = dir.join("input.bam").display().to_string();
    MarkDuplicatesConfig {
        input: input.clone(),
        inputs: vec![input],
        output: dir.join("output.bam").display().to_string(),
        metrics_file: dir.join("metrics.txt").display().to_string(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: false,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: Some(1),
        reference_sequence: None,
        tmp_dir: Some(dir.join("scratch").display().to_string()),
    }
}

fn header(second_library: &str) -> bam::Header {
    let mut header = bam::Header::new();
    header.push_record(
        HeaderRecord::new(b"HD")
            .push_tag(b"VN", "1.6")
            .push_tag(b"SO", "coordinate"),
    );
    header.push_record(
        HeaderRecord::new(b"SQ")
            .push_tag(b"SN", "chr1")
            .push_tag(b"LN", 10_000_000),
    );
    for (id, library) in [("rg1", "lib1"), ("rg2", second_library)] {
        header.push_record(
            HeaderRecord::new(b"RG")
                .push_tag(b"ID", id)
                .push_tag(b"SM", "sample")
                .push_tag(b"LB", library)
                .push_tag(b"PL", "ILLUMINA"),
        );
    }
    header
}

fn record(
    name: &[u8],
    group: &str,
    flags: u16,
    position: i64,
    mate: i64,
    quality: u8,
) -> bam::Record {
    let mut record = bam::Record::new();
    let cigar = CigarString(vec![Cigar::Match(20)]);
    record.set(name, Some(&cigar), &[b'A'; 20], &[quality; 20]);
    record.set_flags(flags);
    record.set_tid(0);
    record.set_pos(position);
    record.set_mapq(60);
    record.set_mtid(if flags & 1 != 0 { 0 } else { -1 });
    record.set_mpos(mate);
    record.set_insert_size(if flags & 1 == 0 {
        0
    } else if flags & 64 != 0 {
        120
    } else {
        -120
    });
    record.push_aux(b"RG", Aux::String(group)).unwrap();
    record
}

fn write_pair_fixture(
    config: &MarkDuplicatesConfig,
    second_library: &str,
    shared_name: bool,
    padding: usize,
    unmapped: bool,
) {
    let mut writer =
        bam::Writer::from_path(&config.input, &header(second_library), bam::Format::Bam).unwrap();
    writer
        .set_compression_level(bam::CompressionLevel::Level(1))
        .unwrap();
    let first_name = if shared_name {
        b"shared".as_slice()
    } else {
        b"z-winner".as_slice()
    };
    let second_name = if shared_name {
        b"shared".as_slice()
    } else {
        b"a-duplicate".as_slice()
    };
    for (flags, position, mate) in [(99, 100, 200), (147, 200, 100)] {
        writer
            .write(&record(first_name, "rg1", flags, position, mate, 40))
            .unwrap();
        writer
            .write(&record(second_name, "rg2", flags, position, mate, 20))
            .unwrap();
    }
    for index in 0..padding {
        writer
            .write(&record(
                format!("padding-{index:08}").as_bytes(),
                "rg1",
                0,
                1000 + index as i64 * 30,
                -1,
                30,
            ))
            .unwrap();
    }
    if unmapped {
        let mut record = bam::Record::new();
        record.set(b"unplaced", None, &[b'A'; 20], &[30; 20]);
        record.push_aux(b"RG", Aux::String("rg1")).unwrap();
        writer.write(&record).unwrap();
    }
}

fn output_flags(config: &MarkDuplicatesConfig) -> Vec<u16> {
    bam::Reader::from_path(&config.output)
        .unwrap()
        .records()
        .map(|record| record.unwrap().flags())
        .collect()
}

#[test]
fn external_plan_accepts_coordinate_ties_and_unplaced_tail_above_compact_cutoff() {
    let dir = tempdir().unwrap();
    let config = config(dir.path());
    write_pair_fixture(
        &config,
        "lib1",
        false,
        COMPACT_MARKDUP_MAX_RECORDS + 1,
        true,
    );
    let summary = try_run_external_plan(&config)
        .unwrap()
        .expect("ordinary coordinate order must stay on the external path");
    let flags = output_flags(&config);
    assert_eq!(&flags[..4], &[99, 1123, 147, 1171]);
    assert_eq!(flags.last(), Some(&4));
    assert_eq!(flags.len(), COMPACT_MARKDUP_MAX_RECORDS + 6);
    assert_eq!(summary.duplicate_pair_records, 2);
    assert_eq!(summary.unmapped_records, 1);
    assert!(
        fs::read_dir(config.tmp_dir.as_ref().unwrap())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn repeated_names_across_groups_in_one_library_are_distinct_templates() {
    let dir = tempdir().unwrap();
    let mut config = config(dir.path());
    config.tag_duplicate_set_members = true;
    write_pair_fixture(&config, "lib1", true, 0, false);
    assert!(
        try_run_small_single_bam_compact_plan(&config)
            .unwrap()
            .is_none()
    );
    let summary = run(&config).unwrap();
    assert_eq!(output_flags(&config), [99, 1123, 147, 1171]);
    assert_eq!(summary.read_pairs_examined, 2);
    assert_eq!(summary.duplicate_pair_records, 2);
    for record in bam::Reader::from_path(&config.output).unwrap().records() {
        let record = record.unwrap();
        assert_eq!(record.aux(b"DS").unwrap(), Aux::I32(2));
        assert_eq!(record.aux(b"DI").unwrap(), Aux::I32(0));
    }
}

#[test]
fn repeated_names_across_libraries_are_not_paired_together() {
    let dir = tempdir().unwrap();
    let config = config(dir.path());
    write_pair_fixture(&config, "lib2", true, 0, false);
    assert!(
        try_run_single_bam_no_duplicate_fast_path(&config)
            .unwrap()
            .is_none()
    );
    assert!(
        try_run_small_single_bam_compact_plan(&config)
            .unwrap()
            .is_none()
    );
    let summary = run(&config).unwrap();
    assert_eq!(output_flags(&config), [99, 99, 147, 147]);
    assert_eq!(summary.duplicate_pair_records, 0);
    let text = fs::read_to_string(&config.metrics_file).unwrap();
    assert!(text.lines().any(|line| line.starts_with("lib1\t")));
    assert!(text.lines().any(|line| line.starts_with("lib2\t")));
}

#[test]
fn genuine_coordinate_reversal_does_not_claim_external_eligibility() {
    let dir = tempdir().unwrap();
    let config = config(dir.path());
    {
        let mut writer =
            bam::Writer::from_path(&config.input, &header("lib1"), bam::Format::Bam).unwrap();
        writer
            .write(&record(b"later", "rg1", 0, 200, -1, 30))
            .unwrap();
        writer
            .write(&record(b"earlier", "rg1", 0, 100, -1, 30))
            .unwrap();
    }
    assert!(try_run_external_plan(&config).unwrap().is_none());
    assert!(!Path::new(&config.output).exists());
    assert!(
        fs::read_dir(config.tmp_dir.as_ref().unwrap())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn no_duplicate_probe_stops_at_its_record_budget() {
    let dir = tempdir().unwrap();
    let config = config(dir.path());
    {
        let mut header = bam::Header::new();
        header.push_record(
            HeaderRecord::new(b"HD")
                .push_tag(b"VN", "1.6")
                .push_tag(b"SO", "coordinate"),
        );
        header.push_record(
            HeaderRecord::new(b"SQ")
                .push_tag(b"SN", "chr1")
                .push_tag(b"LN", 10_000_000),
        );
        header.push_record(
            HeaderRecord::new(b"RG")
                .push_tag(b"ID", "rg1")
                .push_tag(b"LB", "lib1"),
        );
        let mut writer = bam::Writer::from_path(&config.input, &header, bam::Format::Bam).unwrap();
        for index in 0..=COMPACT_MARKDUP_MAX_RECORDS {
            writer
                .write(&record(
                    format!("unique-{index:08}").as_bytes(),
                    "rg1",
                    0,
                    index as i64 * 30,
                    -1,
                    30,
                ))
                .unwrap();
        }
    }
    assert!(
        try_run_single_bam_no_duplicate_fast_path(&config)
            .unwrap()
            .is_none()
    );
    assert!(!Path::new(&config.output).exists());
    assert!(!Path::new(&temp_hts_output_path(&config.output)).exists());
}

#[test]
fn decision_replay_rejects_every_truncated_record_boundary() {
    let decision = ExternalDecision {
        flags: EXTERNAL_DECISION_DUPLICATE,
        duplicate_set_size: None,
        duplicate_set_index: None,
    };
    let mut bytes = 7_u64.to_be_bytes().to_vec();
    bytes.extend(encode_external_decision_payload(&decision));
    assert!(
        read_external_duplicate_decision(&mut Cursor::new(&[]))
            .unwrap()
            .is_none()
    );
    for end in 1..bytes.len() {
        assert!(
            read_external_duplicate_decision(&mut Cursor::new(&bytes[..end])).is_err(),
            "accepted truncation at {end}"
        );
    }
    assert_eq!(
        read_external_duplicate_decision(&mut Cursor::new(&bytes)).unwrap(),
        Some((7, decision))
    );
}

#[test]
fn buffered_decision_replay_batches_underlying_reads() {
    struct Counted {
        inner: Cursor<Vec<u8>>,
        reads: usize,
    }
    impl io::Read for Counted {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            self.inner.read(buffer)
        }
    }
    let decision = ExternalDecision {
        flags: EXTERNAL_DECISION_DUPLICATE,
        duplicate_set_size: None,
        duplicate_set_index: None,
    };
    let mut data = Vec::new();
    for ordinal in 0..2000_u64 {
        data.extend_from_slice(&ordinal.to_be_bytes());
        data.extend(encode_external_decision_payload(&decision));
    }
    let mut reader = BufReader::new(Counted {
        inner: Cursor::new(data),
        reads: 0,
    });
    for ordinal in 0..2000 {
        assert_eq!(
            read_external_duplicate_decision(&mut reader).unwrap(),
            Some((ordinal, decision))
        );
    }
    assert!(
        read_external_duplicate_decision(&mut reader)
            .unwrap()
            .is_none()
    );
    assert!(
        reader.get_ref().reads < 10,
        "decision replay should batch thousands of short reads"
    );
}
