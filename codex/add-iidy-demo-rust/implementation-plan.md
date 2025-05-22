# Implementation Plan for iidy demo (Rust)

- [x] **CLI wiring**
  - Extend `Commands` enum with `Demo(DemoArgs)` (already exists) and ensure `demo` subcommand includes `--timescaling` option.
  - Hook command in `main.rs` to call new function `demo::run`.

- [x] **Demo module**
  - Create `src/demo.rs` implementing `run(args)`.
  - Parse YAML script using `serde_yaml` into `DemoScript` struct containing optional `files` map and `demo` Vec<Command>.
  - Support command variants: `SetEnv`, `Banner`, `Sleep`, `Shell`, `Silent`.
  - Provide environment merging and timescaling fields.

- [x] **File extraction**
  - Create temp directory via `tempfile::tempdir` and write files before running commands.
  - Use `tokio::fs` or std `fs` for writes.

- [x] **Command execution**
  - For each command:
    - `SetEnv`: extend env hashmap.
    - `Shell`: print using colored typing effect then execute via `Command` with `/bin/bash -c` and check exit code.
    - `Silent`: same as shell without printing.
    - `Sleep`: use `tokio::time::sleep` scaled by factor.
    - `Banner`: draw a banner with terminal width using ANSI colors.

- [x] **Cleanup**
  - Remove temp dir after execution regardless of success.

- [ ] **Testing**
  - Add basic unit test driving a minimal demo script using temp files.

