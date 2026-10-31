#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TURBO_PICARD_CONDA_PREFIX:?set TURBO_PICARD_CONDA_PREFIX to a conda env with upstream picard and samtools}"
export TURBO_PICARD_CONDA_PREFIX

./tools/verify_basic_accelerationstatus_parity.sh
./tools/verify_basic_capabilities_parity.sh
./tools/verify_basic_doctor_parity.sh
./tools/verify_basic_explain_parity.sh
./tools/verify_basic_mcp_parity.sh
./tools/verify_basic_trial_parity.sh

export TURBO_PICARD_REQUIRE_NATIVE=1
./tools/verify_basic_cram_parity.sh
./tools/verify_markdup_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_parity.sh
./tools/verify_gatk_mito_bam_parity.sh
./tools/verify_gatk_mito_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_cram_parity.sh
./tools/validate_gatk_mito_cram_evidence.sh
./tools/verify_basic_picard_parity.sh
./tools/verify_basic_sortsam_parity.sh
./tools/verify_basic_cleansam_parity.sh
./tools/verify_basic_mergesamfiles_parity.sh
./tools/verify_basic_buildbamindex_parity.sh
./tools/verify_basic_samtofastq_parity.sh
./tools/verify_basic_fastqtosam_parity.sh
./tools/verify_basic_addorreplacereadgroups_parity.sh
./tools/verify_basic_alignmentmetrics_parity.sh
./tools/verify_basic_collectbasedistributionbycycle_parity.sh
./tools/verify_basic_collectgcbiasmetrics_parity.sh
./tools/verify_basic_collecthsmetrics_parity.sh
./tools/verify_basic_qualityyield_parity.sh
./tools/verify_basic_collectwgsmetrics_parity.sh
./tools/verify_basic_qualityscoredistribution_parity.sh
./tools/verify_basic_meanqualitybycycle_parity.sh
./tools/verify_basic_collectinsertsizemetrics_parity.sh
./tools/verify_basic_collectmultiplemetrics_parity.sh
./tools/verify_basic_fixmateinformation_parity.sh
./tools/verify_basic_intervallisttools_parity.sh
./tools/verify_basic_revertsam_parity.sh
./tools/verify_basic_setnmmdanduqtags_parity.sh
./tools/verify_basic_validatesamfile_parity.sh
./tools/verify_basic_liftovervcf_parity.sh
./tools/verify_basic_createdict_parity.sh
./tools/verify_basic_normalizefasta_parity.sh
./tools/verify_basic_bedtointervallist_parity.sh
./tools/verify_basic_viewsam_parity.sh
./tools/verify_basic_replacesamheader_parity.sh
./tools/verify_basic_updatevcfsequencedictionary_parity.sh
./tools/verify_basic_gathervcfs_parity.sh
./tools/verify_basic_sortvcf_parity.sh
./tools/verify_basic_mergevcfs_parity.sh

echo "parity gate passed"
