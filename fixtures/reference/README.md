# Reference fixtures for parity and CRAM I/O

| File | Use |
| --- | --- |
| `chr1.fa` | Synthetic MarkDuplicates and `verify_basic_cram_parity.sh` shard |
| `chrM.fa` | GATK NA12878 mitochondrial real-data and CRAM evidence (`Homo_sapiens_assembly38.mt_only.fasta` from GATK commit `e8c49f600b06c658e0fa9bf67256340ebb46bc48`) |

Set `TURBO_PICARD_REFERENCE` or pass `R=` / `--reference-fasta` when reading or writing CRAM.