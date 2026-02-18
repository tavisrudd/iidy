# Enabling rust-analyzer LSP in Claude Code

Date: 2026-02-18

## What this gives you

Nine LSP operations exposed as a built-in tool: `goToDefinition`,
`findReferences`, `hover`, `documentSymbol`, `workspaceSymbol`,
`goToImplementation`, `prepareCallHierarchy`, `incomingCalls`, `outgoingCalls`.

Also: automatic diagnostics injected into Read/Write tool results after
editing `.rs` files.

## Prerequisites

rust-analyzer must be on PATH:
```bash
rustup component add rust-analyzer
```

## Setup

### 1. Enable the LSP tool

Add to `~/.claude/settings.json`:
```json
"env": {
  "ENABLE_LSP_TOOL": "1"
}
```

Without this env var, the LSP server runs (diagnostics work) but the
navigation tool is not exposed.

### 2. Create a local plugin marketplace

This avoids any runtime dependency on third-party repos. Structure:

```
~/.claude/local-plugins/
  .claude-plugin/marketplace.json
  rust-analyzer/
    .claude-plugin/plugin.json
    .lsp.json
```

**`.claude-plugin/marketplace.json`**:
```json
{
  "name": "local-plugins",
  "owner": { "name": "local" },
  "plugins": [
    {
      "name": "rust-analyzer",
      "description": "Rust language server for code intelligence",
      "version": "1.0.0",
      "source": "./rust-analyzer"
    }
  ]
}
```

**`rust-analyzer/.claude-plugin/plugin.json`**:
```json
{
  "name": "rust-analyzer",
  "description": "Rust language server",
  "version": "1.0.0"
}
```

**`rust-analyzer/.lsp.json`** (direct, one rust-analyzer per session):
```json
{
  "rust": {
    "command": "rust-analyzer",
    "extensionToLanguage": {
      ".rs": "rust"
    }
  }
}
```

**`rust-analyzer/.lsp.json`** (shared via ra-multiplex):
```json
{
  "rust": {
    "command": "ra-multiplex",
    "args": ["client"],
    "extensionToLanguage": {
      ".rs": "rust"
    }
  }
}
```

The direct approach spawns a separate rust-analyzer per Claude session
(~3-4GB each). The ra-multiplex approach shares one instance across all
sessions. See the ra-multiplex section below.

### 3. Register the marketplace

Add to `~/.claude/settings.json`:
```json
"extraKnownMarketplaces": {
  "local-plugins": {
    "source": {
      "source": "directory",
      "path": "/home/tavis/.claude/local-plugins"
    }
  }
},
"enabledPlugins": {
  "rust-analyzer@local-plugins": true
}
```

Claude Code also registers the marketplace in
`~/.claude/plugins/known_marketplaces.json` automatically after the first
`/plugin install rust-analyzer@local-plugins`.

### 4. Restart Claude Code

Verify with `/plugin` -- should show `rust-analyzer@local-plugins` as enabled
with no errors. rust-analyzer takes ~30s to index this crate on first startup
(~3-4GB RAM).

## ra-multiplex (optional, recommended)

[ra-multiplex](https://github.com/pr2502/ra-multiplex) v0.2.6 (MIT) is an
LSP multiplexer that lets multiple clients share one rust-analyzer per
workspace. Without it, each Claude session spawns its own rust-analyzer.

Install and pin to v0.2.6:
```bash
cargo install ra-multiplex@0.2.6
```

Create a systemd user service at `~/.config/systemd/user/ra-multiplex.service`:
```ini
[Unit]
Description=ra-multiplex LSP server (rust-analyzer multiplexer)

[Service]
Environment=PATH=%h/.cargo/bin:%h/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/usr/bin:/bin
ExecStart=%h/.cargo/bin/ra-multiplex server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Enable and start:
```bash
systemctl --user enable --now ra-multiplex.service
```

Optionally create `~/.config/ra-multiplex/config.toml` (empty file is fine)
to suppress the missing-config warning.

## Notes

- The official `rust-analyzer-lsp@claude-plugins-official` plugin is a stub
  (README only, no `.lsp.json`). Uninstall it if present.
- Based on the [boostvolt/claude-code-lsps](https://github.com/boostvolt/claude-code-lsps)
  plugin structure, cloned locally to avoid any external runtime dependency.
- ra-multiplex is archived on GitHub, moved to Codeberg as `lspmux`. The
  v0.2.6 crates.io release is the final MIT version.
- The systemd service needs an explicit PATH because NixOS systemd user
  sessions don't inherit the shell PATH.
- Claude Code caches plugin files at install time under
  `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/`. Edits to the
  source directory are not picked up until the plugin is reinstalled
  (`/plugin install rust-analyzer@local-plugins`) or the cache is edited
  directly.
