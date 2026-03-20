# rusty-obsidian-mcp

Rust MCP server for Obsidian vaults. Wraps the Obsidian CLI (v1.12+) and exposes 37 tools, 3 resources, and 3 prompts over the Model Context Protocol. Supports stdio, local HTTP, and ngrok tunnel transports.

Requires Obsidian to be running with the CLI enabled (Settings > General > Command line interface).

## Quick start

```bash
cp .env.example .env
# Edit .env: set OBSIDIAN_VAULT=YourVaultName

# stdio (for Cursor, Claude Desktop)
cargo run

# local HTTP server
cargo run --features http -- --http

# ngrok tunnel (internet-accessible)
cargo run --features tunnel -- --tunnel
```

## CLI arguments

| Argument | Description |
|---|---|
| `--http` | Start local HTTP server instead of stdio |
| `--tunnel [domain]` | Start ngrok tunnel. Optional: pass your stable domain |
| `-v, --vault <name>` | Vault name (overrides OBSIDIAN_VAULT env) |
| `-p, --port <port>` | HTTP/tunnel port (default: 8000) |
| `--host <host>` | HTTP bind address (default: 127.0.0.1) |
| `--api-key <key>` | API key for HTTP/tunnel auth (overrides MCP_API_KEY env) |
| `--no-auth` | Disable API key authentication |
| `--skip-health-check` | Skip startup CLI health check |

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `OBSIDIAN_VAULT` | No | Vault name. Auto-detected if single vault. |
| `OBSIDIAN_BIN` | No | Path to obsidian binary (default: `obsidian`) |
| `OBSIDIAN_TIMEOUT` | No | CLI timeout in seconds (default: 30) |
| `ENABLE_DANGEROUS_TOOLS` | No | Enable eval_js and execute_command |
| `MCP_API_KEY` | No | API key for HTTP/tunnel. Auto-generated if unset. |
| `NGROK_AUTHTOKEN` | For tunnel | ngrok auth token |
| `NGROK_DOMAIN` | No | Stable ngrok domain (default: random URL) |

## Build features

- `cargo build` -- stdio only, lean binary
- `cargo build --features http` -- adds local HTTP server
- `cargo build --features tunnel` -- adds ngrok tunnel (includes http)

## MCP client configuration

Cursor / Claude Desktop (stdio):

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "path/to/rusty-obsidian-mcp",
      "env": { "OBSIDIAN_VAULT": "MyVault" }
    }
  }
}
```

Remote via ngrok:

```
MCP endpoint: https://your-name.ngrok-free.app/mcp
Authorization: Bearer <api_key>
```
