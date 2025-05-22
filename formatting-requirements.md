# iidy Output Formatting Requirements

This document summarises the behaviour of the original `iidy-js` command line
in terms of coloured and aligned output.  It will guide further work on the
Rust port.

## General Notes

* Colours are applied using ANSI escape codes via the `cli-color` library.
* When `--color=never` is supplied or when stdout is not a TTY, colours are
  disabled by setting the `NO_COLOR` environment variable.
* Many commands right-pad or left-pad values so that table style output lines up
  correctly.  Padding widths are calculated at runtime based on the data being
  printed.
* Timestamps are formatted with `dateformat` and coloured using xterm colour 253
  (light gray).

## Command Requirements

### list-stacks
* Prints a header `Creation/Update Time, Status, Name[, Tags]` in dim grey.
* Each stack line contains:
  1. The creation or last update time, padded to 24 characters and coloured
     light grey.
  2. The stack status padded to the longest status value.  Status strings are
     coloured: failures bright red, in‑progress yellow, completed green and
     skipped blue.
  3. A lifecycle icon (`🔒`, `∞`, `♺`) coloured grey when present.
  4. The stack name coloured based on environment: production red,
     integration xterm 75, development xterm 194.  StackSet instances append the
     StackSet name in grey.
  5. Optional comma separated tags coloured grey.
* If a stack has a `StackStatusReason` containing `FAILED` it is printed on the
  next line indented two spaces in grey.

### describe-stack / watch-stack
* Section headings such as `Stack Details:` or `Live Stack Events:` are bold and
  coloured white (xterm 255).
* Output fields are printed in two columns.  The label column width is 24
  characters and the value column follows.
* Resource statuses use the same colouring rules as `list-stacks`.
* When showing stack events the code calculates the terminal width and wraps the
  reason text accordingly.

### delete-stack / create-stack / update-stack
* After a command completes a summary is printed.  `Success` is shown on a green
  background, `Failure` on red.  The overall command return code corresponds to
  the CloudFormation operation status.

### drift
* Drifted resources are listed with the logical id coloured light grey and the
  drift status in red.  Property differences are indented by three spaces.

## Implementation Notes for Rust

* Use the `anstyle` crate (already available through `clap`) to apply colours.
* When stdout is not a TTY or the `NO_COLOR` env var is present all formatting
  functions should return plain strings.
* A `ColorScheme` struct should hold the styles.  Defaults match the TypeScript
  implementation but values can be overridden via environment variables such as
  `IIDY_COLOR_STATUS_FAILED` etc.
* Padding and width calculations mirror the logic found in `src/cfn/formatting.ts`.
