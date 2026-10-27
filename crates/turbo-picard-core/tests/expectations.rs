//! Jane Street-style expect tests for the shared argument and I/O boundary.
//!
//! Each assertion shows the whole observed value instead of a hand-picked
//! field, so a changed default, ordering rule, or error message is visible in
//! the diff. Run `UPDATE_EXPECT=1 cargo test -p turbo-picard-core --test
//! expectations` to accept an intentional change.

use expect_test::expect;
use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter};
use turbo_picard_core::hts_io::{
    is_hts_container_input, is_sam_text_input, output_format_for_path, path_extension_lower,
    path_format,
};
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_core::picard_args::{
    CommandArgError, PicardArgError, PicardArgs, normalize_picard_args,
    normalize_picard_args_for_command,
};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn normalize(command: &'static str, values: &[&str]) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_for_command(command, &strings(values))
}

fn markdup_config(values: &[&str]) -> Result<MarkDuplicatesConfig, String> {
    let parsed = normalize("MarkDuplicates", values).map_err(|error| error.to_string())?;
    MarkDuplicatesConfig::try_from_args(&parsed).map_err(|error| error.to_string())
}

fn describe<T: std::fmt::Debug>(result: Result<T, CommandArgError>) -> String {
    match result {
        Ok(value) => format!("ok {value:?}"),
        Err(error) => format!("error {error}"),
    }
}

#[test]
fn normalized_argument_map_is_visible() {
    let parsed = normalize(
        "MarkDuplicates",
        &[
            "I=in.bam",
            "I=second.bam",
            "O=out.bam",
            "-M",
            "metrics.txt",
            "-SO",
            "coordinate",
            "-ASO",
            "coordinate",
            "-DS",
            "SUM_OF_BASE_QUALITIES",
            "MAX_FILE_HANDLES=128",
            "PG=null",
            "R=ref.fa",
            "COMMENT=first pass",
            "REMOVE_DUPLICATES=true",
        ],
    )
    .expect("arguments parse");

    expect![[r#"
        PicardArgs {
            command: "MarkDuplicates",
            values: {
                "ASSUME_SORT_ORDER": [
                    "coordinate",
                ],
                "COMMENT": [
                    "first pass",
                ],
                "DUPLICATE_SCORING_STRATEGY": [
                    "SUM_OF_BASE_QUALITIES",
                ],
                "INPUT": [
                    "in.bam",
                    "second.bam",
                ],
                "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP": [
                    "128",
                ],
                "METRICS_FILE": [
                    "metrics.txt",
                ],
                "OUTPUT": [
                    "out.bam",
                ],
                "PROGRAM_RECORD_ID": [
                    "null",
                ],
                "REFERENCE_SEQUENCE": [
                    "ref.fa",
                ],
                "REMOVE_DUPLICATES": [
                    "true",
                ],
                "SORT_ORDER": [
                    "coordinate",
                ],
            },
        }"#]]
    .assert_eq(&format!("{parsed:#?}"));
}

#[test]
fn command_specific_alias_table_is_visible() {
    let cases: &[(&'static str, &str)] = &[
        ("MarkDuplicates", "M=value"),
        ("MergeSamFiles", "AS=true"),
        ("CollectInsertSizeMetrics", "M=0.05"),
        ("CollectWgsMetrics", "MQ=30"),
        ("CollectWgsMetrics", "Q=25"),
        ("SamToFastq", "F2=r2.fastq"),
        ("FastqToSam", "F2=r2.fastq"),
        ("RevertSam", "OQ=true"),
        ("ValidateSamFile", "M=SUMMARY"),
        ("CreateSequenceDictionary", "AS=GRCh38"),
        ("IntervalListTools", "SI=other.interval_list"),
        ("SortVcf", "SD=ref.dict"),
        ("CollectHsMetrics", "BAIT=baits.interval_list"),
    ];

    let rendered = cases
        .iter()
        .map(|(command, argument)| {
            let parsed = normalize(command, &[argument]).expect("alias resolves");
            let key = parsed.keys().next().expect("one key");
            format!("{command} {argument} -> {key}")
        })
        .collect::<Vec<_>>()
        .join("\n");

    expect![[r#"
        MarkDuplicates M=value -> METRICS_FILE
        MergeSamFiles AS=true -> ASSUME_SORTED
        CollectInsertSizeMetrics M=0.05 -> MINIMUM_PCT
        CollectWgsMetrics MQ=30 -> MINIMUM_MAPPING_QUALITY
        CollectWgsMetrics Q=25 -> MINIMUM_BASE_QUALITY
        SamToFastq F2=r2.fastq -> SECOND_END_FASTQ
        FastqToSam F2=r2.fastq -> FASTQ2
        RevertSam OQ=true -> RESTORE_ORIGINAL_QUALITIES
        ValidateSamFile M=SUMMARY -> MODE
        CreateSequenceDictionary AS=GRCh38 -> GENOME_ASSEMBLY
        IntervalListTools SI=other.interval_list -> SECOND_INPUT
        SortVcf SD=ref.dict -> SEQUENCE_DICTIONARY
        CollectHsMetrics BAIT=baits.interval_list -> BAIT_INTERVALS"#]]
    .assert_eq(&rendered);
}

#[test]
fn argument_errors_are_visible() {
    let cases: &[(&str, &[&str])] = &[
        ("positional", &["in.bam"]),
        ("positional after options", &["--INPUT=in.bam", "OUTPUT"]),
        ("missing long value", &["--INPUT"]),
        ("missing short value", &["-I"]),
        ("flag-like value", &["--INPUT", "-X"]),
        ("empty key", &["=value"]),
        ("empty long key", &["--=value"]),
        ("negative integer value", &["-MQ", "-1"]),
        ("negative float value", &["--Q", "-.5"]),
    ];

    let rendered = cases
        .iter()
        .map(|(label, args)| {
            let result = normalize("CollectWgsMetrics", args);
            match result {
                Ok(parsed) => format!("{label}: ok {parsed:?}"),
                Err(error) => format!("{label}: error {error}"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    expect![[r#"
        positional: error unexpected positional Picard argument: in.bam
        positional after options: error unexpected positional Picard argument: OUTPUT
        missing long value: error missing value for Picard argument: INPUT
        missing short value: error missing value for Picard argument: INPUT
        flag-like value: error missing value for Picard argument: INPUT
        empty key: error empty Picard argument key in 
        empty long key: error empty Picard argument key in 
        negative integer value: ok PicardArgs { command: "CollectWgsMetrics", values: {"MINIMUM_MAPPING_QUALITY": ["-1"]} }
        negative float value: ok PicardArgs { command: "CollectWgsMetrics", values: {"MINIMUM_BASE_QUALITY": ["-.5"]} }"#]].assert_eq(&rendered);
}

#[test]
fn markduplicates_config_is_visible() {
    let config = markdup_config(&[
        "I=in.bam",
        "I=second.bam",
        "O=out.bam",
        "M=metrics.txt",
        "REMOVE_DUPLICATES=true",
        "REMOVE_SEQUENCING_DUPLICATES=true",
        "ASSUME_SORTED=true",
        "-ASO",
        "coordinate",
        "VALIDATION_STRINGENCY=SILENT",
        "QUIET=true",
        "CREATE_INDEX=true",
        "CREATE_MD5_FILE=true",
        "ADD_PG_TAG_TO_READS=false",
        "TAG_DUPLICATE_SET_MEMBERS=true",
        "-DS",
        "SUM_OF_BASE_QUALITIES",
        r"READ_NAME_REGEX=^[^:]+:(\d+):(\d+):(\d+)$",
        "TAGGING_POLICY=OpticalOnly",
        "BARCODE_TAG=BC",
        "READ_ONE_BARCODE_TAG=RX",
        "READ_TWO_BARCODE_TAG=QX",
        "CLEAR_DT=false",
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE=100",
        "COMPRESSION_LEVEL=5",
        "R=ref.fa",
        "TMP_DIR=scratch",
    ])
    .expect("config validates");

    expect![[r#"
        MarkDuplicatesConfig {
            input: "in.bam",
            inputs: [
                "in.bam",
                "second.bam",
            ],
            output: "out.bam",
            metrics_file: "metrics.txt",
            remove_duplicates: true,
            remove_sequencing_duplicates: true,
            assume_sorted: true,
            assume_sort_order: Some(
                "coordinate",
            ),
            validation_stringency: Some(
                "SILENT",
            ),
            quiet: true,
            create_index: true,
            create_md5_file: true,
            add_pg_tag_to_reads: false,
            tag_duplicate_set_members: true,
            duplicate_scoring_strategy: Some(
                "SUM_OF_BASE_QUALITIES",
            ),
            read_name_regex: Some(
                "^[^:]+:(\\d+):(\\d+):(\\d+)$",
            ),
            tagging_policy: Some(
                "OpticalOnly",
            ),
            barcode_tag: Some(
                "BC",
            ),
            read_one_barcode_tag: Some(
                "RX",
            ),
            read_two_barcode_tag: Some(
                "QX",
            ),
            clear_dt: false,
            optical_duplicate_pixel_distance: Some(
                100,
            ),
            compression_level: Some(
                5,
            ),
            reference_sequence: Some(
                "ref.fa",
            ),
            tmp_dir: Some(
                "scratch",
            ),
        }"#]]
    .assert_eq(&format!("{config:#?}"));
}

#[test]
fn markduplicates_config_errors_are_visible() {
    let cases: &[(&str, &[&str])] = &[
        ("missing input", &["O=out.bam", "M=metrics.txt"]),
        ("missing output", &["I=in.bam", "M=metrics.txt"]),
        ("missing metrics", &["I=in.bam", "O=out.bam"]),
        (
            "duplicate scalar",
            &["I=in.bam", "O=out.bam", "O=second.bam", "M=metrics.txt"],
        ),
        (
            "invalid boolean",
            &["I=in.bam", "O=out.bam", "M=metrics.txt", "QUIET=yes"],
        ),
        (
            "unsupported option",
            &["I=in.bam", "O=out.bam", "M=metrics.txt", "NOT_AN_OPTION=1"],
        ),
        (
            "unsupported scoring strategy",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "DUPLICATE_SCORING_STRATEGY=OTHER",
            ],
        ),
        (
            "unsupported sort order",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "ASSUME_SORT_ORDER=queryname",
            ],
        ),
        (
            "invalid read-name regex",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "READ_NAME_REGEX=^[^:]+:(\\d+):(\\d+)$",
            ],
        ),
        (
            "invalid tagging policy",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "TAGGING_POLICY=Some",
            ],
        ),
        (
            "invalid tag name",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "BARCODE_TAG=TOO_LONG",
            ],
        ),
        (
            "compression level above 9",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "COMPRESSION_LEVEL=10",
            ],
        ),
        (
            "negative pixel distance",
            &[
                "I=in.bam",
                "O=out.bam",
                "M=metrics.txt",
                "OPTICAL_DUPLICATE_PIXEL_DISTANCE=-1",
            ],
        ),
    ];

    let rendered = cases
        .iter()
        .map(|(label, args)| match markdup_config(args) {
            Ok(config) => format!("{label}: ok {config:?}"),
            Err(error) => format!("{label}: error {error}"),
        })
        .collect::<Vec<_>>()
        .join("\n");

    expect![[r#"
        missing input: error missing required MarkDuplicates argument: INPUT
        missing output: error missing required MarkDuplicates argument: OUTPUT
        missing metrics: error missing required MarkDuplicates argument: METRICS_FILE
        duplicate scalar: error duplicate scalar MarkDuplicates argument: OUTPUT
        invalid boolean: error invalid boolean for MarkDuplicates argument QUIET: yes
        unsupported option: error unsupported MarkDuplicates argument: NOT_AN_OPTION
        unsupported scoring strategy: error unsupported MarkDuplicates argument: DUPLICATE_SCORING_STRATEGY=OTHER
        unsupported sort order: error unsupported MarkDuplicates argument: ASSUME_SORT_ORDER=queryname
        invalid read-name regex: error unsupported MarkDuplicates argument: READ_NAME_REGEX=^[^:]+:(\d+):(\d+)$
        invalid tagging policy: error unsupported MarkDuplicates argument: TAGGING_POLICY=Some
        invalid tag name: error unsupported MarkDuplicates argument: BARCODE_TAG=TOO_LONG
        compression level above 9: error unsupported MarkDuplicates argument: COMPRESSION_LEVEL=10
        negative pixel distance: error unsupported MarkDuplicates argument: OPTICAL_DUPLICATE_PIXEL_DISTANCE=-1"#]].assert_eq(&rendered);
}

#[test]
fn path_format_decisions_are_visible() {
    let paths = [
        "reads.bam",
        "READS.BAM",
        "reads.sam",
        "reads.cram",
        "reads.bam.tmp",
        "reads",
        "reads.vcf",
        "reads.fastq.gz",
        ".bam",
        "",
    ];

    let rendered = paths
        .iter()
        .map(|path| {
            format!(
                "{path:?}: extension={:?} format={:?} container={} sam_text={}",
                path_extension_lower(path),
                path_format(path),
                is_hts_container_input(path),
                is_sam_text_input(path),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    expect![[r#"
        "reads.bam": extension=Some("bam") format=Some(Bam) container=true sam_text=false
        "READS.BAM": extension=Some("bam") format=Some(Bam) container=true sam_text=false
        "reads.sam": extension=Some("sam") format=Some(Sam) container=false sam_text=true
        "reads.cram": extension=Some("cram") format=Some(Cram) container=true sam_text=false
        "reads.bam.tmp": extension=Some("tmp") format=None container=false sam_text=false
        "reads": extension=None format=None container=false sam_text=false
        "reads.vcf": extension=Some("vcf") format=None container=false sam_text=false
        "reads.fastq.gz": extension=Some("gz") format=None container=false sam_text=false
        ".bam": extension=None format=None container=false sam_text=false
        "": extension=None format=None container=false sam_text=false"#]]
    .assert_eq(&rendered);
}

#[test]
fn output_format_errors_are_visible() {
    let cases = [
        "out.bam", "out.BAM", "out.sam", "out.cram", "out.vcf", "out",
    ];

    let rendered = cases
        .iter()
        .map(
            |path| match output_format_for_path(path, "MarkDuplicates") {
                Ok(format) => format!("{path}: ok {format:?}"),
                Err(error) => format!("{path}: error {error}"),
            },
        )
        .collect::<Vec<_>>()
        .join("\n");

    expect![[r#"
        out.bam: ok Bam
        out.BAM: ok Bam
        out.sam: ok Sam
        out.cram: ok Cram
        out.vcf: error unsupported MarkDuplicates output format for out.vcf; expected .sam, .bam, or .cram
        out: error unsupported MarkDuplicates output format for out; expected .sam, .bam, or .cram"#]].assert_eq(&rendered);
}

#[test]
fn external_sort_output_is_visible() {
    let dir = tempfile::tempdir().expect("tempdir exists");
    let mut config = ExternalSortConfig::new(dir.path());
    config.max_records_in_ram = 2;
    config.merge_fan_in = 2;
    let mut sorter = ExternalSorter::new(config).expect("sorter starts");

    for (key, payload) in [
        (b"b".as_slice(), "b-1"),
        (b"a".as_slice(), "a-1"),
        (b"b".as_slice(), "b-2"),
        (b"a".as_slice(), "a-2"),
    ] {
        sorter
            .push(key.to_vec(), payload.as_bytes().to_vec())
            .expect("item is buffered");
    }

    let mut emitted = Vec::new();
    let metrics = sorter
        .finish_into(|item| {
            emitted.push(format!(
                "{}={}",
                String::from_utf8(item.key).expect("utf-8 key"),
                String::from_utf8(item.payload).expect("utf-8 payload"),
            ));
            Ok(())
        })
        .expect("sort completes");

    let rendered = format!(
        "emitted={emitted:?}\nspills={}\nrun_count={}\nmax_resident_records={}",
        metrics.spills, metrics.run_count, metrics.max_resident_records,
    );

    expect![[r#"
        emitted=["a=a-1", "a=a-2", "b=b-1", "b=b-2"]
        spills=2
        run_count=2
        max_resident_records=2"#]]
    .assert_eq(&rendered);
}

#[test]
fn typed_command_args_are_visible() {
    let parsed = normalize(
        "MarkDuplicates",
        &[
            "I=in.bam",
            "I=second.bam",
            "O=out.bam",
            "M=metrics.txt",
            "REMOVE_DUPLICATES=yes",
            "OPTICAL_DUPLICATE_PIXEL_DISTANCE=-5",
        ],
    )
    .expect("arguments parse");
    let typed = parsed.typed();

    let rendered = [
        format!("command={}", typed.command()),
        format!("inputs={}", describe(typed.required_values("INPUT"))),
        format!("output={}", describe(typed.required_string("OUTPUT"))),
        format!(
            "missing={}",
            describe(typed.required_string("REFERENCE_SEQUENCE"))
        ),
        format!(
            "invalid boolean={}",
            describe(typed.optional_bool("REMOVE_DUPLICATES"))
        ),
        format!(
            "invalid quantity={}",
            describe(typed.optional_u32("OPTICAL_DUPLICATE_PIXEL_DISTANCE"))
        ),
        format!(
            "unknown={}",
            describe(parsed.reject_unknown(&["INPUT", "OUTPUT", "METRICS_FILE"]))
        ),
    ]
    .join("\n");

    expect![[r#"
        command=MarkDuplicates
        inputs=ok ["in.bam", "second.bam"]
        output=ok "out.bam"
        missing=error missing required MarkDuplicates argument: REFERENCE_SEQUENCE
        invalid boolean=error invalid boolean for MarkDuplicates argument REMOVE_DUPLICATES: yes
        invalid quantity=error unsupported MarkDuplicates argument OPTICAL_DUPLICATE_PIXEL_DISTANCE=-5
        unknown=error unsupported MarkDuplicates argument: OPTICAL_DUPLICATE_PIXEL_DISTANCE"#]].assert_eq(&rendered);
}

#[test]
fn generic_normalization_keeps_picard_aliases() {
    let parsed = normalize_picard_args(&strings(&[
        "-I",
        "in.bam",
        "-O",
        "out.bam",
        "-M",
        "metrics.txt",
        "-CO",
        "comment",
    ]))
    .expect("arguments parse");

    expect![[r#"
        PicardArgs {
            command: "Picard",
            values: {
                "COMMENT": [
                    "comment",
                ],
                "INPUT": [
                    "in.bam",
                ],
                "METRICS_FILE": [
                    "metrics.txt",
                ],
                "OUTPUT": [
                    "out.bam",
                ],
            },
        }"#]]
    .assert_eq(&format!("{parsed:#?}"));
}
