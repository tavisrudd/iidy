# iidy demo Requirements

From studying the `iidy-js` implementation, the demo feature allows the CLI to execute scripted demo sessions defined in YAML. Key points:

- A demo script is a YAML file processed through iidy's preprocessor (`transform`). It may include `$imports` and `$defs` for variables.
- The script has a `files` map with relative paths as keys and file contents as string values. These files are extracted to a temporary directory before commands run.
- The `demo` list contains commands executed sequentially. Supported commands:
  - `setenv`: merge values into the process environment for subsequent commands.
  - `banner`: display a highlighted banner across the terminal width.
  - `sleep`: pause for given seconds. A `--timescaling` factor scales sleeps.
  - `shell`: run a shell command while printing it character by character in red preceded by `Shell Prompt >`. Fails on non-zero exit status.
  - `silent`: run a shell command without printing but still fails on error.
- During execution each shell command runs inside the temporary directory created for the demo. Environment variable `PKG_SKIP_EXECPATH_PATCH=yes` is set to avoid pkg issues.
- At completion the temporary directory is removed. The command exits with code 0 on success and non-zero on failures.

The Rust implementation should replicate this behaviour using equivalent crates:
- Parse YAML using serde_yaml and apply the existing preprocessing logic (not yet implemented in Rust). For now we can read the YAML directly without preprocessing but the design should account for future integration.
- Use `tempfile` to create a temporary directory.
- Use `std::process::Command` to execute shell commands with `shell = true` using `/bin/bash`.
- Provide CLI option `demo <demoscript>` with optional `--timescaling` (default 1).
- Implement banners using terminal size (from `term_size` crate) and ANSI color codes.
- Print typed commands character by character with small delay (`tokio::time::sleep`), scaled by `--timescaling`.

Initial version can skip advanced preprocessing features but must support the command types listed above and the temporary file extraction.

