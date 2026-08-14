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

- Live package: `turbo-picard` 0.1.11
- Current repo release candidate: 0.1.12
- Install: `python3 -m pip install turbo-picard`
- Current PyPI files: Linux x86_64 wheel, macOS Apple Silicon wheel, and source distribution
- Bioconda is not yet a current `0.1.12` install path: the existing open PR
  #65922 covers older `0.1.10` metadata. Use PyPI `0.1.11` or the published
  container until the exact `0.1.12` tag, archive, review, and publication are
  complete.
- Trial helper: `turbo-picard trial <PicardCommand> ...`
- Benchmark evidence in the repo reports 32/32 parity-checked commands, 87.47x
  geometric mean speedup, and 261.75x top speedup versus Picard 3.4.0
- Recommended first commands: `MarkDuplicates`, `SortSam`, `SamToFastq`,
  `FastqToSam`, `FixMateInformation`, `BuildBamIndex`, and metrics commands
