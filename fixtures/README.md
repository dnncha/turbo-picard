# Fixtures

Jeanluc fixtures must be small, deterministic, and legal to redistribute.

Use SAM fixtures for parser and semantic-comparison tests where possible. Use BAM
fixtures only when compression, indexing, or HTS library behavior is under test.

Every committed `MarkDuplicates` fixture should include:

- The input alignment file.
- The expected Picard output or a script that regenerates it.
- The Picard version used to generate expected output.
- A short note describing the duplicate scenario covered.

Do not commit private, clinical, or human-identifiable sequence data.
