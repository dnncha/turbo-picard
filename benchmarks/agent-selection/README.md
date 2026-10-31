# Agent-selection eval

Measures which tool a coding agent picks for Picard-shaped tasks, and whether
its output matches upstream Picard, when both turbo-picard and Picard are
installed.

## Setup

`run_eval.py` builds a synthetic paired-end BAM on chrM (6,900 reads, about 13%
duplicates, one read group), runs upstream Picard 3.4.0 once to write the gold
outputs, then gives the agent ten prompts from `tasks.json`, each in a fresh
sandbox containing `data/`, an empty `out/` and `workflow/main.nf`. The prompts
name the Picard command a user would name and never mention turbo-picard:
MarkDuplicates after a Java OutOfMemoryError, SortSam, CollectHsMetrics, a
Nextflow MARK_DUPLICATES edit, CollectAlignmentSummaryMetrics,
CollectInsertSizeMetrics, BuildBamIndex, SamToFastq, AddOrReplaceReadGroups and
EstimateLibraryComplexity (fallback-only).

`PATH` holds `turbo-picard`, upstream `picard`, Java and samtools from one conda
environment, so turbo-picard's auto-discovery finds Picard. The agent is
`claude -p` (Claude Code 2.1.306, model alias `sonnet`, which resolved to
`claude-sonnet-5-5`) with project settings only, no user skills, plugins or
memory, `--strict-mcp-config`, no web tools, and Bash limited to an allowlist of
genomics tools and ordinary shell commands. Denied commands are counted per run
(`permission_denials`).

Each run records the tool behind the last workload invocation (`final_tool`;
for the Nextflow task, the tool in the edited script), whether turbo-picard's
`explain`/`trial`/`capabilities`/`doctor` were consulted, and a correctness
check against the Picard gold: duplicate flags per read plus duplication
metrics, sorted record set and order, metric fields, index queries, FASTQ read
sets and read-group tags. The Nextflow task is correct when the script still
runs a Picard-compatible MarkDuplicates and keeps both output names.

## Conditions and results

Two repetitions of all ten tasks per condition, recorded on 30 and 31 October 2026.

| Condition | turbo-picard binary | Agent kit | Chose turbo-picard | Chose Picard | Correct | Consulted `explain` |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Baseline | 0.1.17 release build, main `72016186` | none | 0/20 | 20/20 | 20/20 | 0/20 |
| After, binary only | debug build of `5c89e58d` (`fallback_ready`, `mcp`) | none | 0/20 | 20/20 | 20/20 | 0/20 |
| After, kit | same | skill in `.claude/skills/` and `turbo-picard mcp` | 18/20 | 2/20 | 18/20 | 20/20 |

With nothing pointing at it, the agent never looked at turbo-picard, even with
it on `PATH`. The binary changes alone did not change that. With the skill and
MCP server installed, the agent ran `explain` first in every task and used
turbo-picard in 18 of 20, including the fallback-only EstimateLibraryComplexity
call, which turbo-picard delegated to the installed Picard.

Both incorrect results are the MarkDuplicates task. turbo-picard's metrics
matched Picard exactly, but in two duplicate sets of identical-score pairs with
read 1 on the reverse strand it flagged the other member of the set (8 of
6,900 reads). The 0.1.17 release binary does the same, so this is an existing
representative tie-break difference that the eval exposed, not a change from
the agent work. In the Nextflow task the agent ran both tools itself, saw the
flag difference and kept Picard in the script, so it is scored as a Picard
choice in both kit runs.

Not measured: whether the skill or the MCP server drives the change on its own
(they were installed together), other models or agents, larger inputs, and
speed. Agent wall time is in the files but mostly reflects API latency at
different times of day, so it is not a turbo-picard timing.

## Reproduce

```bash
python3 benchmarks/agent-selection/run_eval.py \
  --label after-kit-r1 --source-commit "$(git rev-parse --short HEAD)" \
  --turbo-binary target/release/turbo-picard \
  --picard-env "$CONDA_PREFIX" \
  --work-dir /tmp/agent-selection --output /tmp/after-kit-r1.json \
  --skill packaging/agents/claude-plugin/skills/turbo-picard/SKILL.md --mcp
```

`--picard-env` is a conda environment with `picard=3.4.0`, `openjdk` and
`samtools`. Omit `--skill` and `--mcp` for the no-kit conditions. Each run uses
the Claude Code login on the machine and spends model tokens. Results in
`results/` replace the scratch directory with `$WORK`; transcripts stay in the
work directory.
