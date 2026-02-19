# Handoff: Investigate Linker Memory Usage on Test Builds

**Date**: 2026-02-18
**Status**: Resolved

## Problem

Running `cargo test template_hash` while multiple Opus sub-agents were
active caused memory pressure severe enough that the user had to SIGTERM
the build before it could OOM the machine. The linker was working on
the `tree_sitter_debug` test binary at the time.

Note: cargo compiles all test binaries even when filtering by test name.

## Resolution

Mold was already configured in `.cargo/config.toml` and installed via nix
(mold 2.40.4). The config was working locally but broke GitHub CI, which
doesn't have mold installed.

Fix (commit d84ebb3): moved the mold rustflag from the project-level
`.cargo/config.toml` to `~/.cargo/config.toml` (user-level). Cargo merges
configs from both locations, so mold is still used locally but CI uses the
default linker.

The `jobs = 12` cap remains in the project config to limit parallel linking
memory pressure.

### Current config

**`.cargo/config.toml`** (committed):
```toml
[build]
jobs = 12
```

**`~/.cargo/config.toml`** (local only):
```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```
