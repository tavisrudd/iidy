# Handoff: Investigate Linker Memory Usage on Test Builds

**Date**: 2026-02-18

## Problem

Running `cargo test template_hash` while multiple Opus sub-agents were
active caused memory pressure severe enough that the user had to SIGTERM
the build before it could OOM the machine. The linker was working on
the `tree_sitter_debug` test binary at the time.

Note: cargo compiles all test binaries even when filtering by test name.

## Context

We thought we had resolved linker memory issues previously (likely by
switching to `mold` or adjusting linker settings). This may have
regressed, or the default linker (`ld`) may still be in use.

## Investigation Steps

1. **Check current linker config**:
   - `cat .cargo/config.toml` -- is `mold` configured as the linker?
   - If not, check `~/.cargo/config.toml` for global settings
   - Check if `mold` is installed: `which mold`

2. **Check if `tree_sitter_debug` is unusually large**:
   - `grep -r tree_sitter_debug tests/`
   - This test binary may pull in the full tree-sitter grammar, inflating link size

3. **Baseline memory usage**:
   - With no other heavy processes, run `make test` and monitor with
     `watch -n1 free -h` in another terminal
   - Note peak memory during linking phase

4. **If mold is not configured**:
   - Add to `.cargo/config.toml`:
     ```toml
     [target.x86_64-unknown-linux-gnu]
     linker = "clang"
     rustflags = ["-C", "link-arg=-fuse-ld=mold"]
     ```
   - Or if using nix, ensure mold is in the dev shell
   - Mold uses significantly less memory than GNU ld during linking

5. **If mold IS configured but still high memory**:
   - Consider splitting large test binaries
   - Check if `tree_sitter_debug` can be consolidated or trimmed

## Priority

Medium -- the build should not be fragile under moderate memory pressure.
