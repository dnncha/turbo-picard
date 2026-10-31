# MCP server

`turbo-picard mcp` is a Model Context Protocol server on stdio. It exposes five
tools that wrap the existing JSON contracts:

| Tool | Wraps | Writes files |
| --- | --- | --- |
| `capabilities` | `turbo-picard capabilities --json [--command C]` | no |
| `explain` | `turbo-picard explain --json C ARGS...` | no |
| `trial` | `turbo-picard trial --json C ARGS...` | no |
| `doctor` | `turbo-picard doctor`, as a JSON object | no |
| `run` | `turbo-picard C ARGS...`, without a shell | yes |

`run` refuses an output argument that names an input or an existing file
unless `overwrite` is true, and `require_native: true` sets
`TURBO_PICARD_REQUIRE_NATIVE=1` for that call.

Client configuration:

```json
{
  "mcpServers": {
    "turbo-picard": { "command": "turbo-picard", "args": ["mcp"] }
  }
}
```

Claude Code: `claude mcp add turbo-picard -- turbo-picard mcp`.
Codex: `codex mcp add turbo-picard -- turbo-picard mcp`.

The server needs a build from the current source or the next release; it is not
in 0.1.17.

## Registry listing

`server.json` is the MCP Registry entry for the PyPI package. The registry
checks PyPI ownership through the `mcp-name: io.github.dnncha/turbo-picard`
line in the README that PyPI shows, so publish only after a release that
contains the server:

```bash
mcp-publisher login github
mcp-publisher publish packaging/mcp/server.json
```

Update both `version` fields to that release first. `tools/test_mcp_listing.py`
checks the listing against the package metadata.
