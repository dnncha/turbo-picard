# Genome Unwoven — isolated public-data build

This branch runs the newly authored public UCSC annotation builder for the Genome Unwoven visualisation requested for cheerfulduck.com. It is NOT a Turbo Picard feature, dependency or release. Do not merge this research branch into main. No private site source, user data or credentials are included. Generated files contain only aggregated public reference-genome annotations and their source checksums.

Canonical application work lives in the Cheerful Duck repository. This isolated branch provides a public-runner execution path after that private repository's runner failed before executing any steps.

Run `python research/genome-unwoven/build_genome.py --output research/genome-unwoven/data` to rebuild. Memory usage and coverage reconciliations are recorded in the generated manifest. Input source hashes must be pinned after the first validated build before treating the snapshot as immutable.
