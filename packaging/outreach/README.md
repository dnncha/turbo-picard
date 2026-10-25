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
- Benchmarks: https://turbo-picard.readthedocs.io/en/latest/benchmarks.html

Ground rules:

- Disclose that you are the author or maintainer.
- Do not paste the same post everywhere. Use the channel-specific version.
- Lead with the concrete trial: choose one slow Picard command, run both tools
  on the same representative input, and compare outputs.
- Avoid replying to old Q&A threads just to promote the project. Reply only when
  the tool directly answers the question, and disclose your affiliation.
- Do not ask friends to upvote or comment.
- Be ready for questions about parity scope, unsupported options, fallback
  behavior, Linux packaging, and real-data validation.

Current facts to keep consistent:

- Release source and publication target: `turbo-picard` 0.1.17
- Install: `python3 -m pip install turbo-picard`
- Target PyPI files: Linux x86_64 and ARM64 wheels, macOS Intel and Apple Silicon wheels, and source distribution
- Bioconda accepted the initial recipes on 4 October 2026. The 25 October
  channel receipt lists `0.1.15`; the `0.1.16` main-package and shim updates
  remain open as PRs #69890 and #69891. The local recipes target `0.1.17`.
  Verify the pinned `0.1.17` PyPI wheel or container before publishing install
  guidance, and recheck the channel before advertising a Bioconda version.
- Trial helper: `turbo-picard trial <PicardCommand> ...`
- The historical 14 August 2026 suite reports 32/32 parity-checked small-fixture
  command comparisons, 84.52x geometric mean and 272.12x maximum saved ratio
  versus Picard 3.4.0. Startup contributes; keep the date and fixture scope
  beside any quoted ratio. These are not current-release or WGS predictions.
- The 0.1.16 MarkDuplicates gate matched eleven bounded synthetic cases.
  Paired CI spill fixtures measured 16–18% lower runtime and 8–13% lower RSS.
  Some native competitors were faster; full output contracts differ.
  Keep that scope and trade-off visible; this is not WGS/cohort evidence.
- Recommended first commands: `MarkDuplicates`, `SortSam`, `SamToFastq`,
  `FastqToSam`, `FixMateInformation`, `BuildBamIndex`, and metrics commands
