use turbo_picard_core::picard_args::{
    PicardArgError, normalize_picard_args, normalize_picard_args_for_command,
};

#[test]
fn normalizes_key_value_arguments() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_long_options() {
    let args = vec![
        "--INPUT".to_string(),
        "in.bam".to_string(),
        "--OUTPUT=out.bam".to_string(),
        "--METRICS_FILE".to_string(),
        "metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_short_picard_options() {
    let args = vec![
        "-I".to_string(),
        "in.bam".to_string(),
        "-O".to_string(),
        "out.bam".to_string(),
        "-M".to_string(),
        "metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_common_markduplicates_short_aliases() {
    let args = vec![
        "-AS".to_string(),
        "true".to_string(),
        "-SO".to_string(),
        "coordinate".to_string(),
        "-ASO".to_string(),
        "coordinate".to_string(),
        "-DS".to_string(),
        "SUM_OF_BASE_QUALITIES".to_string(),
        "-PG".to_string(),
        "null".to_string(),
        "-R".to_string(),
        "reference.fa".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(
        parsed.get("SORT_ORDER").unwrap(),
        &vec!["coordinate".to_string()]
    );
    assert_eq!(
        parsed.get("ASSUME_SORTED").unwrap(),
        &vec!["true".to_string()]
    );
    assert_eq!(
        parsed.get("ASSUME_SORT_ORDER").unwrap(),
        &vec!["coordinate".to_string()]
    );
    assert_eq!(
        parsed.get("DUPLICATE_SCORING_STRATEGY").unwrap(),
        &vec!["SUM_OF_BASE_QUALITIES".to_string()]
    );
    assert_eq!(
        parsed.get("PROGRAM_RECORD_ID").unwrap(),
        &vec!["null".to_string()]
    );
    assert_eq!(
        parsed.get("REFERENCE_SEQUENCE").unwrap(),
        &vec!["reference.fa".to_string()]
    );
}

#[test]
fn normalizes_conflicting_aliases_by_command() {
    let markdup_args = vec!["M=metrics.txt".to_string(), "AS=true".to_string()];
    let markdup = normalize_picard_args_for_command("MarkDuplicates", &markdup_args)
        .expect("MarkDuplicates arguments parse");
    assert_eq!(markdup["METRICS_FILE"], vec!["metrics.txt"]);
    assert_eq!(markdup["ASSUME_SORTED"], vec!["true"]);

    let merge_args = vec!["AS=true".to_string()];
    let merge = normalize_picard_args_for_command("MergeSamFiles", &merge_args)
        .expect("MergeSamFiles arguments parse");
    assert_eq!(merge["ASSUME_SORTED"], vec!["true"]);

    let alignment_args = vec!["LEVEL=ALL_READS".to_string()];
    let alignment =
        normalize_picard_args_for_command("CollectAlignmentSummaryMetrics", &alignment_args)
            .expect("CollectAlignmentSummaryMetrics arguments parse");
    assert_eq!(alignment["METRIC_ACCUMULATION_LEVEL"], vec!["ALL_READS"]);

    let validate_args = vec!["M=SUMMARY".to_string(), "R=ref.fa".to_string()];
    let validate = normalize_picard_args_for_command("ValidateSamFile", &validate_args)
        .expect("ValidateSamFile arguments parse");
    assert_eq!(validate["MODE"], vec!["SUMMARY"]);
    assert_eq!(validate["REFERENCE_SEQUENCE"], vec!["ref.fa"]);

    let dict_args = vec!["AS=GRCh38".to_string(), "REFERENCE=ref.fa".to_string()];
    let dict = normalize_picard_args_for_command("CreateSequenceDictionary", &dict_args)
        .expect("CreateSequenceDictionary arguments parse");
    assert_eq!(dict["GENOME_ASSEMBLY"], vec!["GRCh38"]);
    assert_eq!(dict["REFERENCE_SEQUENCE"], vec!["ref.fa"]);

    let interval_args = vec!["SD=ref.dict".to_string()];
    let interval = normalize_picard_args_for_command("BedToIntervalList", &interval_args)
        .expect("BedToIntervalList arguments parse");
    assert_eq!(interval["SEQUENCE_DICTIONARY"], vec!["ref.dict"]);

    let replace_header_args = vec!["H=header.sam".to_string()];
    let replace_header =
        normalize_picard_args_for_command("ReplaceSamHeader", &replace_header_args)
            .expect("ReplaceSamHeader arguments parse");
    assert_eq!(replace_header["HEADER"], vec!["header.sam"]);

    let update_vcf_args = vec!["SD=ref.dict".to_string()];
    let update_vcf =
        normalize_picard_args_for_command("UpdateVcfSequenceDictionary", &update_vcf_args)
            .expect("UpdateVcfSequenceDictionary arguments parse");
    assert_eq!(update_vcf["SEQUENCE_DICTIONARY"], vec!["ref.dict"]);

    let sort_vcf_args = vec!["D=ref.dict".to_string()];
    let sort_vcf = normalize_picard_args_for_command("SortVcf", &sort_vcf_args)
        .expect("SortVcf arguments parse");
    assert_eq!(sort_vcf["SEQUENCE_DICTIONARY"], vec!["ref.dict"]);

    let insert_args = vec![
        "H=insert.pdf".to_string(),
        "M=0.25".to_string(),
        "LEVEL=ALL_READS".to_string(),
    ];
    let insert = normalize_picard_args_for_command("CollectInsertSizeMetrics", &insert_args)
        .expect("CollectInsertSizeMetrics arguments parse");
    assert_eq!(insert["HISTOGRAM_FILE"], vec!["insert.pdf"]);
    assert_eq!(insert["MINIMUM_PCT"], vec!["0.25"]);
    assert_eq!(insert["METRIC_ACCUMULATION_LEVEL"], vec!["ALL_READS"]);

    let multiple_args = vec!["R=ref.fa".to_string(), "LEVEL=ALL_READS".to_string()];
    let multiple = normalize_picard_args_for_command("CollectMultipleMetrics", &multiple_args)
        .expect("CollectMultipleMetrics arguments parse");
    assert_eq!(multiple["REFERENCE_SEQUENCE"], vec!["ref.fa"]);
    assert_eq!(multiple["METRIC_ACCUMULATION_LEVEL"], vec!["ALL_READS"]);

    let sam_to_fastq_args = vec![
        "F=r1.fastq".to_string(),
        "F2=r2.fastq".to_string(),
        "FU=unpaired.fastq".to_string(),
        "Q=20".to_string(),
        "CLIP_ATTR=XT".to_string(),
        "CLIP_ACT=N".to_string(),
        "CLIP_MIN=3".to_string(),
        "R1_MAX_BASES=90".to_string(),
        "R2_MAX_BASES=80".to_string(),
        "R=ref.fa".to_string(),
    ];
    let sam_to_fastq = normalize_picard_args_for_command("SamToFastq", &sam_to_fastq_args)
        .expect("SamToFastq arguments parse");
    assert_eq!(sam_to_fastq["FASTQ"], vec!["r1.fastq"]);
    assert_eq!(sam_to_fastq["SECOND_END_FASTQ"], vec!["r2.fastq"]);
    assert_eq!(sam_to_fastq["UNPAIRED_FASTQ"], vec!["unpaired.fastq"]);
    assert_eq!(sam_to_fastq["QUALITY"], vec!["20"]);
    assert_eq!(sam_to_fastq["CLIPPING_ATTRIBUTE"], vec!["XT"]);
    assert_eq!(sam_to_fastq["CLIPPING_ACTION"], vec!["N"]);
    assert_eq!(sam_to_fastq["CLIPPING_MIN_LENGTH"], vec!["3"]);
    assert_eq!(sam_to_fastq["READ1_MAX_BASES_TO_WRITE"], vec!["90"]);
    assert_eq!(sam_to_fastq["READ2_MAX_BASES_TO_WRITE"], vec!["80"]);
    assert_eq!(sam_to_fastq["REFERENCE_SEQUENCE"], vec!["ref.fa"]);

    let fixmate_args = vec!["AS=true".to_string(), "MC=false".to_string()];
    let fixmate = normalize_picard_args_for_command("FixMateInformation", &fixmate_args)
        .expect("FixMateInformation arguments parse");
    assert_eq!(fixmate["ASSUME_SORTED"], vec!["true"]);
    assert_eq!(fixmate["ADD_MATE_CIGAR"], vec!["false"]);

    let intervals_args = vec![
        "SI=other.interval_list".to_string(),
        "M=INTERVAL_COUNT".to_string(),
    ];
    let intervals = normalize_picard_args_for_command("IntervalListTools", &intervals_args)
        .expect("IntervalListTools arguments parse");
    assert_eq!(intervals["SECOND_INPUT"], vec!["other.interval_list"]);
    assert_eq!(intervals["SUBDIVISION_MODE"], vec!["INTERVAL_COUNT"]);

    let revert_args = vec!["OQ=true".to_string(), "RHC=false".to_string()];
    let revert =
        normalize_picard_args_for_command("RevertSam", &revert_args).expect("RevertSam args parse");
    assert_eq!(revert["RESTORE_ORIGINAL_QUALITIES"], vec!["true"]);
    assert_eq!(revert["RESTORE_HARDCLIPS"], vec!["false"]);

    let set_tags_args = vec!["R=ref.fa".to_string()];
    let set_tags = normalize_picard_args_for_command("SetNmMdAndUqTags", &set_tags_args)
        .expect("SetNmMdAndUqTags args parse");
    assert_eq!(set_tags["REFERENCE_SEQUENCE"], vec!["ref.fa"]);

    let liftover_args = vec![
        "C=lift.chain".to_string(),
        "R=target.fa".to_string(),
        "WMC=true".to_string(),
    ];
    let liftover = normalize_picard_args_for_command("LiftoverVcf", &liftover_args)
        .expect("LiftoverVcf args parse");
    assert_eq!(liftover["CHAIN"], vec!["lift.chain"]);
    assert_eq!(liftover["REFERENCE_SEQUENCE"], vec!["target.fa"]);
    assert_eq!(liftover["WARN_ON_MISSING_CONTIG"], vec!["true"]);

    let gc_bias_args = vec![
        "R=ref.fa".to_string(),
        "S=summary.txt".to_string(),
        "CHART=gc.pdf".to_string(),
        "WINDOW_SIZE=100".to_string(),
        "MGF=0.0001".to_string(),
    ];
    let gc_bias = normalize_picard_args_for_command("CollectGcBiasMetrics", &gc_bias_args)
        .expect("CollectGcBiasMetrics args parse");
    assert_eq!(gc_bias["REFERENCE_SEQUENCE"], vec!["ref.fa"]);
    assert_eq!(gc_bias["SUMMARY_OUTPUT"], vec!["summary.txt"]);
    assert_eq!(gc_bias["CHART_OUTPUT"], vec!["gc.pdf"]);
    assert_eq!(gc_bias["SCAN_WINDOW_SIZE"], vec!["100"]);
    assert_eq!(gc_bias["MINIMUM_GENOME_FRACTION"], vec!["0.0001"]);

    let wgs_args = vec![
        "MQ=30".to_string(),
        "Q=25".to_string(),
        "CAP=100".to_string(),
    ];
    let wgs = normalize_picard_args_for_command("CollectWgsMetrics", &wgs_args)
        .expect("CollectWgsMetrics args parse");
    assert_eq!(wgs["MINIMUM_MAPPING_QUALITY"], vec!["30"]);
    assert_eq!(wgs["MINIMUM_BASE_QUALITY"], vec!["25"]);
    assert_eq!(wgs["COVERAGE_CAP"], vec!["100"]);

    let fastq_to_sam_args = vec![
        "F1=r1.fastq".to_string(),
        "F2=r2.fastq".to_string(),
        "O=unmapped.sam".to_string(),
        "R=ref.fa".to_string(),
        "RG=rg1".to_string(),
        "SM=sample".to_string(),
        "LB=lib".to_string(),
        "PL=ILLUMINA".to_string(),
        "PU=unit".to_string(),
    ];
    let fastq_to_sam = normalize_picard_args_for_command("FastqToSam", &fastq_to_sam_args)
        .expect("FastqToSam args parse");
    assert_eq!(fastq_to_sam["FASTQ"], vec!["r1.fastq"]);
    assert_eq!(fastq_to_sam["FASTQ2"], vec!["r2.fastq"]);
    assert_eq!(fastq_to_sam["OUTPUT"], vec!["unmapped.sam"]);
    assert_eq!(fastq_to_sam["REFERENCE_SEQUENCE"], vec!["ref.fa"]);
    assert_eq!(fastq_to_sam["READ_GROUP_NAME"], vec!["rg1"]);
    assert_eq!(fastq_to_sam["SAMPLE_NAME"], vec!["sample"]);
    assert_eq!(fastq_to_sam["LIBRARY_NAME"], vec!["lib"]);
    assert_eq!(fastq_to_sam["PLATFORM"], vec!["ILLUMINA"]);
    assert_eq!(fastq_to_sam["PLATFORM_UNIT"], vec!["unit"]);

    let read_groups_args = vec!["R=ref.fa".to_string()];
    let read_groups =
        normalize_picard_args_for_command("AddOrReplaceReadGroups", &read_groups_args)
            .expect("AddOrReplaceReadGroups args parse");
    assert_eq!(read_groups["REFERENCE_SEQUENCE"], vec!["ref.fa"]);
}

#[test]
fn rejects_positional_arguments() {
    let args = vec!["in.bam".to_string()];

    let err = normalize_picard_args(&args).unwrap_err();

    assert_eq!(
        err,
        PicardArgError::UnexpectedPositional("in.bam".to_string())
    );
}

#[test]
fn rejects_long_option_without_value() {
    let args = vec!["--INPUT".to_string()];

    let err = normalize_picard_args(&args).unwrap_err();

    assert_eq!(err, PicardArgError::MissingValue("INPUT".to_string()));
}
