# CLAUDE.md

## Essential Context for Auto-Compact Recovery

When context auto-compacts, read these documents first to understand current state and progress:

### **Primary Design Documents (Read First)**
1. **`notes/2025-06-17-data-driven-output-architecture.md`** - Core architecture design with data structures, renderer traits, fixture-based testing approach (sections 2063-2305)
2. **`notes/2025-06-17-console-output-modes.md`** - Output modes specification (Interactive, Plain, JSON, TUI) with exact requirements
3. **`notes/2025-06-17-complete-iidy-implementation-spec.md`** - Pixel-perfect iidy-js implementation spec with exact colors, spacing, constants (color codes at lines 912-918)

### **Implementation Status & Progress**
4. **`notes/2025-06-17-data-driven-output-architecture-implementation.md`** - Current implementation status, completed phases, testing strategy analysis
5. **Current Todo List** - Use `TodoRead` tool to see current tasks and priorities

### **Key Implementation Details**
- **Theme System**: `src/output/theme.rs` with exact iidy-js colors (NOT old src/terminal.rs/color.rs)
- **Data Structures**: `src/output/data.rs` - Complete OutputData enum matching design spec
- **Renderers**: `src/output/renderers/` - Interactive (pixel-perfect), Plain (CI-friendly)  
- **Testing**: Three-layer strategy using `insta` snapshots and YAML fixtures
- **Architecture**: Data-driven separation of collection from presentation, supports mode switching

### **Current Testing Status (✅ COMPLETED)**
- **Layer 1 Unit Tests**: 14/14 passing (`tests/output_unit_tests.rs`)
- **Layer 2 Integration Tests**: 21/21 passing (`tests/output_renderer_snapshots.rs`, `tests/fixture_validation_tests.rs`)
- **Fixture System**: Complete with expected outputs for Interactive, Plain, JSON modes
- **Test Infrastructure**: Output capture, ANSI validation, fixture loading all working
- **Ready For**: Layer 3 DynamicOutputManager testing OR Phase 2 pixel-perfect output matching

### **Important Notes**
- Ignore old `src/terminal.rs` and `src/color.rs` modules (from pre-design spike)
- TUI mode removed from current scope (implement later)
- CLI supports `--theme` (Dark/Light/HighContrast/Auto) and `--color` (Always/Never/Auto)
- All tests must be offline/deterministic using fixture data

---

## General requirements 
- Work to completion of your goal with 100% of tests passing, no regressions, and no new code warnings. 
- Don't stop to brag or celebrate. Keep going until you have completely reached the goal and completed all tasks.
- Use your Write tool to write files rather than echo or cat.
- 96% or 98% or even 99.6% tests passing is not completion of the goal. 100% is. 
- Do not claim that failing tests are edge cases or not important. That is for the user to determine.

## Development Commands
- Use the standard cargo stuff. 
- Do not use `rustc` directly. Use cargo.
- Use our local @tmp/ dir instead of the system level /tmp
- Never `git checkout HEAD -- <file>`, `git reset`, or `git restore` without making
  a backup of the uncommitted changes and asking for user confirmation.

## Coding Standards
- use meaningful variable and fn names and omit useless comments. If a
  fn's purpose is clear there is no need for comment above it unless
  we are documenting it for the public api.
- comment only the non-obvious
- keep public APIs small. Do not bloat them or re-export what doesn't need exporting.

## Testing
- run `cargo check --lib --tests --bins --benches` for a fast sanity check
- **All tests**: `cargo nextest r --color=never --hide-progress-bar`
- **Snapshot testing**: All example templates in `example-templates/` are automatically tested using `insta`
- Run tests: `cargo test --test example_templates_snapshots`
- Only the user may accept snapshot changes unless they explicitly tell you to and if valid: `cargo insta accept`, but only if the change is value and not a regression.
- Rather than creating adhoc rust binaries or tests not in tests/, just use the existing test infrastructure.
- Do not reward hack by commenting out tests or fudging to make them
  pass. Our goal is working software not tests that pretend to pass.

## Proof of Concepts (POCs)
- **POCs binary**: `cargo run --bin iidy-pocs <demo-name>`
- **Available demos**: `theme-demo`, `spinner-demo`, `ratatui-demo`
- **Location**: All POC code is in `src/pocs/` directory
- **Purpose**: Demonstrations and experimental features for iidy
- See `src/pocs/README.md` for detailed documentation

## Coverage Reporting
- **Quick coverage**: `make coverage-quick`
- **HTML report**: `make coverage-html` (generates `tarpaulin-report.html`)
- **CI coverage**: `make coverage-ci` (70% threshold)
- **Full documentation**: See [docs/COVERAGE.md](docs/COVERAGE.md)

## Project Documentation
See [notes/index.md](notes/index.md) for an overview of all design documents and implementation plans.

## Security
See [docs/SECURITY.md](docs/SECURITY.md) for comprehensive documentation on the YAML import system security model, including restrictions on remote template imports and base path derivation for relative imports.

## Architecture Overview

This is a Rust rewrite of `iidy` https://github.com/unbounce/iidy, a CloudFormation deployment tool. The project follows a modular structure:

### Core Components

- **CLI Layer** (`src/cli.rs`): Complete command-line interface using `clap`, supporting 20+ CloudFormation operations with AWS-specific options and environment-based configuration
- **AWS Integration** (`src/aws.rs`): AWS SDK configuration and credential management 
- **CloudFormation Operations** (`src/cfn/`): Individual modules for each CloudFormation operation (create, update, delete, describe, watch, etc.)
- **Stack Configuration** (`src/stack_args.rs`): YAML-based stack configuration parsing with support for parameters, tags, capabilities, and other CloudFormation options
- **Template Preprocessing** (`src/yaml/`)

### Key Design Patterns

- **Async/Tokio Runtime**: All AWS operations use async/await with Tokio runtime created in `main.rs`
- **Offline testing**: The code is architected so it can be tested offline without connection to AWS:
  - AWS api operations are separate from output formatting and console IO code
  - Test fixtures are used to drive the latter.
- **Error Handling**: Uses `anyhow` for error propagation throughout the codebase, except in `src/yaml` where we generate custom errors which help / debug info for the user.
- **Environment-based Configuration**: Global `--environment` flag loads AWS profiles, regions, and other settings
- **Clap Command Structure**: Extensive use of derive macros for CLI with custom styling and shell completion support

### CloudFormation Operations

The `src/cfn/` modules implement AWS CloudFormation operations:
- Stack lifecycle: create, update, delete, create-or-update
- Change sets: create and execute changesets
- Monitoring: watch stack progress, describe drift
- Utilities: estimate costs, get templates, list instances

Some operations are currently stubs (`todo!()`).

### Configuration Files

- **stack-args.yaml**: Primary configuration file for CloudFormation stacks, supporting parameters, tags, capabilities, IAM roles, and other CloudFormation options
- **Cargo.toml**: Uses AWS SDK v1, clap v4 for CLI, serde for YAML parsing, and tokio for async runtime

### Shell Completion

Generate completions via: `cargo run -- completion <shell>`

## YAML Preprocessing Language Porting Notes

Based on the upstream iidy documentation and implementation, the YAML preprocessing system needs to be ported from TypeScript to Rust.

## YAML Tag Notes
- Yaml !Tags can't be nested directly like !Foo !Bar. You must instead do
!Foo
  - !Bar

## File Cleanup
- When I ask you to clean up temp or .bak files always read my instructions carefully and only remove the exact pattern I specifiy. If I say *.rs.bak, don't delete *.bak!
