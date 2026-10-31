# Agent kit

These files tell coding agents how to choose `turbo-picard` for Picard-shaped
work: run `turbo-picard explain --json` on the exact call, run natively when the
call is inside native scope, and otherwise let turbo-picard delegate to upstream
Picard when `fallback_ready` is true.

| Agent | File | Install |
| --- | --- | --- |
| Claude Code (plugin: skill and MCP server) | [`claude-plugin/`](claude-plugin/) | `/plugin install turbo-picard --marketplace dnncha/turbo-picard` (Claude Code 2.1.275 or later), or `/plugin marketplace add dnncha/turbo-picard` then `/plugin install turbo-picard@turbo-picard` |
| Claude Code (skill only) | [`claude-plugin/skills/turbo-picard/SKILL.md`](claude-plugin/skills/turbo-picard/SKILL.md) | copy to `.claude/skills/turbo-picard/SKILL.md` in a project or `~/.claude/skills/turbo-picard/SKILL.md` |
| Codex and other `AGENTS.md` readers | [`codex/AGENTS.md`](codex/AGENTS.md) | append to the project's `AGENTS.md` |
| Cursor | [`cursor/turbo-picard.mdc`](cursor/turbo-picard.mdc) | copy to `.cursor/rules/turbo-picard.mdc` |

The skill and snippets need `turbo-picard` on `PATH`
(`python3 -m pip install turbo-picard`). For every Picard command to work,
install upstream Picard beside it, for example
`mamba install -c conda-forge -c bioconda turbo-picard picard=3.4.0 openjdk`.
The plugin's MCP server and the `fallback_ready` field need 0.1.18 or later;
with an older install the skill falls back
to reading `fallback_command`.

Copy the skill straight from GitHub:

```bash
mkdir -p .claude/skills/turbo-picard
curl -fsSL https://raw.githubusercontent.com/dnncha/turbo-picard/main/packaging/agents/claude-plugin/skills/turbo-picard/SKILL.md \
  -o .claude/skills/turbo-picard/SKILL.md
```

`tools/test_agent_kit.py` keeps the files consistent. The measured effect of
the kit on tool choice is recorded in
[`benchmarks/agent-selection/`](../../benchmarks/agent-selection/README.md).
