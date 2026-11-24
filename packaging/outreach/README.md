# turbo-picard outreach bundle

This bundle contains draft posts for communities that may care about faster
Picard-compatible commands for existing genomics workflows.

Suggested order:

1. Seqera Community Show & Tell
2. nf-core Slack, if you are already in the workspace or have a concrete module
   question
3. Biostars, using the tool/announcement framing
4. r/bioinformatics, after checking the pinned "before you post" thread
5. Rust community forum announcements
6. Hacker News Show HN, only on a day when you can answer comments
7. Personal LinkedIn, Mastodon, or Bluesky posts

Core links:

- GitHub: https://github.com/dnncha/turbo-picard
- PyPI: https://pypi.org/project/turbo-picard/
- Docs: https://turbo-picard.readthedocs.io/en/latest/
- Quickstart: https://turbo-picard.readthedocs.io/en/latest/quickstart.html
- Evaluation playbook: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- Evidence: https://turbo-picard.readthedocs.io/en/latest/evidence/

Ground rules:

- Disclose that you are the author or maintainer.
- Do not paste the same post everywhere. Use the channel-specific version.
- Lead with the concrete trial: choose one slow Picard command, run both tools
  on the same representative input, and compare outputs.
- Avoid replying to old Q&A threads just to promote the project. Reply only when
  the tool directly answers the question, and disclose your affiliation.
- Do not ask friends to upvote or comment.
- Be ready for questions about parity scope, temp-disk trade-offs, fallback
  behavior, Linux packaging, and real-data validation.

Current facts to keep consistent:

- Release source and publication target: `turbo-picard` 0.1.21
- Native scope: 35 Picard commands plus 6 Turbo Picard utilities; 86 commands
  delegate to upstream Picard when configured. Accepts both classic Picard
  `KEY=VALUE` flags and GATK4 `--kebab-case` / bare boolean flags.
- Install: `python3 -m pip install turbo-picard==0.1.21` or
  `docker run --rm ghcr.io/dnncha/turbo-picard:0.1.21 --version`
- Target PyPI files: Linux x86_64 wheels, macOS Intel and Apple Silicon wheels,
  and source distribution (Linux ARM64 builds from sdist or Bioconda).
- Bioconda accepted the initial recipes on 4 October 2026. Check the current
  Bioconda channel index before advertising a Bioconda version; use a pinned
  PyPI wheel or GHCR container for `0.1.21`.
- Trial and verification helpers: `turbo-picard trial <PicardCommand> ...` and
  `turbo-picard mcp` (six stdio MCP tools, including isolated `verify`).
- 30x GRCh38 whole-genome benchmark (`HG02675`, `ERR3242389`, 740,367,510
  records, 8 vCPU / 50 GB RAM, single run, 23 November 2026):
  - `MarkDuplicates`: 59m 01s / 355 MB RSS vs Picard 3.4.0's 3h 30m 07s /
    34.4 GB RSS (3.6x faster, 97x less memory), matching Picard metrics,
    duplicate count (79,289,617), and SAM columns 1-11 MD5 after #87, and
    matching `DT`, `DS`, `DI`, and `PG` tags across chr21's 11.4M records after
    #88. Trade-off: 99.2 GB vs 23.3 GB peak temp disk (4.3x more, tracked in #86).
  - `CollectWgsMetrics`: 5m 08s / 352 MB RSS vs Picard 3.4.0's 2h 13m 55s /
    4.0 GB RSS (26x faster, 11x less memory), matching every metric field and
    histogram row.
- The historical 14 August 2026 suite reports 32/32 parity-checked small-fixture
  command comparisons, 84.52x geometric mean and 272.12x maximum saved ratio
  versus Picard 3.4.0. Startup contributes; keep the date and fixture scope
  beside any quoted ratio. These are not whole-genome predictions.
- Recommended first commands: `MarkDuplicates`, `CollectWgsMetrics`,
  `CollectMultipleMetrics`, `SortSam`, `SamToFastq`, `FastqToSam`,
  `FixMateInformation`, and `BuildBamIndex`.
