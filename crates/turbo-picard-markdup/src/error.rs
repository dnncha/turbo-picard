use thiserror::Error;
use turbo_picard_core::hts_io::HtsIoError;
use turbo_picard_core::sort_error::SortError;

#[derive(Debug, Error)]
pub enum ExternalPlanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sort(#[from] SortError),
    #[error("external MarkDuplicates pair length is too large")]
    PairLengthTooLarge,
    #[error("external MarkDuplicates pair identities disagree")]
    PairIdentitiesDisagree,
    #[error("external MarkDuplicates QNAME length is too large")]
    QnameLengthTooLarge,
    #[error("external MarkDuplicates record payload has trailing bytes")]
    TrailingPayloadBytes,
    #[error("external MarkDuplicates template key disagrees with its payload")]
    TemplateKeyDisagrees,
    #[error("external MarkDuplicates barcode length is too large")]
    BarcodeLengthTooLarge,
    #[error("external MarkDuplicates barcode marker is invalid")]
    InvalidBarcodeMarker,
    #[error("external MarkDuplicates payload length overflow")]
    PayloadLengthOverflow,
    #[error("external MarkDuplicates record payload is truncated")]
    TruncatedPayload,
    #[error("external MarkDuplicates template key is truncated")]
    TruncatedTemplateKey,
    #[error("external MarkDuplicates template marker is invalid")]
    InvalidTemplateMarker,
    #[error("external MarkDuplicates read-group key is too large")]
    ReadGroupKeyTooLarge,
    #[error("absent read group has a nonempty template key")]
    AbsentReadGroupWithKey,
    #[error("external MarkDuplicates template length overflow")]
    TemplateLengthOverflow,
    #[error("external MarkDuplicates read-group key is truncated")]
    TruncatedReadGroupKey,
    #[error("external MarkDuplicates decision payload has invalid length")]
    InvalidDecisionPayloadLength,
    #[error("external MarkDuplicates decision flags are invalid")]
    InvalidDecisionFlags,
    #[error("external MarkDuplicates duplicate-set metadata is incomplete")]
    IncompleteDuplicateSetMetadata,
    #[error("external MarkDuplicates duplicate-set sizes conflict")]
    ConflictingDuplicateSetSizes,
    #[error("external MarkDuplicates duplicate-set indices conflict")]
    ConflictingDuplicateSetIndices,
    #[error("external MarkDuplicates decision key has invalid length")]
    InvalidDecisionKeyLength,
    #[error("external MarkDuplicates mates must have complementary first/second flags")]
    MatesNotComplementary,
    #[error("external MarkDuplicates decision ordinals are not monotonic")]
    DecisionOrdinalsNotMonotonic,
    #[error("external MarkDuplicates decision ordinal exceeds replay record count")]
    DecisionOrdinalExceedsReplay,
}

impl From<ExternalPlanError> for SortError {
    fn from(error: ExternalPlanError) -> Self {
        match error {
            ExternalPlanError::Sort(inner) => inner,
            other => Self::Sink(other.to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum MarkDuplicatesError {
    #[error("unsupported MarkDuplicates input format for {0}; expected SAM, BAM, or CRAM input")]
    UnsupportedInputFormat(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Htslib(#[from] rust_htslib::errors::Error),
    #[error(transparent)]
    HtsIo(#[from] HtsIoError),
    #[error(transparent)]
    Sort(#[from] SortError),
    #[error(transparent)]
    ExternalPlan(#[from] ExternalPlanError),
    #[error("malformed SAM at line {line_number}: {reason}")]
    MalformedSam { line_number: usize, reason: String },
    #[error("MarkDuplicates requires INPUT")]
    MissingInput,
    #[error("more than 4,294,967,294 distinct molecular barcodes are not supported")]
    TooManyBarcodes,
    #[error("more than 4,294,967,295 distinct libraries are not supported")]
    TooManyLibraries,
    #[error("too many BAM records")]
    TooManyRecords,
    #[error("MarkDuplicates replay record count differs from external plan")]
    ReplayCountDiffersFromExternalPlan,
    #[error("MarkDuplicates replay read more records than its compact plan")]
    ReplayReadMoreThanCompactPlan,
    #[error("MarkDuplicates replay read fewer records than its compact plan")]
    ReplayReadFewerThanCompactPlan,
    #[error("MarkDuplicates compact plan is missing an optical-duplicate decision")]
    MissingOpticalDuplicateDecision,
    #[error("invalid READ_NAME_REGEX: {0}")]
    InvalidReadNameRegex(#[source] regex::Error),
    #[error("MarkDuplicates input sequence dictionaries differ: {input}")]
    SequenceDictionariesDiffer { input: String },
    #[error("MarkDuplicates input sequence dictionaries have conflicting M5 for {name}: {input}")]
    ConflictingSequenceChecksum { name: String, input: String },
    #[error("non-UTF8 temporary input path")]
    NonUtf8TemporaryPath,
    #[error("MarkDuplicates read group is missing ID")]
    ReadGroupMissingId,
    #[error("MarkDuplicates duplicate read group ID: {id}")]
    DuplicateReadGroupId { id: String },
}
