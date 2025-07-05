# CLAUDE.md

## 🔧 CURRENT WORK CONTEXT

**Current Task**: Available for new tasks

### **ARCHITECTURE FOUNDATION (Reference)**
- **`notes/2025-06-17-data-driven-output-architecture.md`** - Core architecture design
- **`notes/2025-06-17-console-output-modes.md`** - Output modes specification
- **`notes/2025-06-17-complete-iidy-implementation-spec.md`** - Pixel-perfect iidy-js spec

### **Key Implementation Details**
- **Theme System**: `src/output/theme.rs` with exact iidy-js colors (NOT old src/terminal.rs/color.rs)
- **Data Structures**: `src/output/data.rs` - Complete OutputData enum matching design spec
- **Renderers**: `src/output/renderers/` - Interactive (pixel-perfect), Plain (CI-friendly)  
- **Testing**: Three-layer strategy using `insta` snapshots and YAML fixtures
- **Architecture**: Data-driven separation of collection from presentation, supports mode switching

### **Important Notes**
- Ignore old `src/terminal.rs` and `src/color.rs` modules (from pre-design spike)
- All tests must be offline/deterministic using fixture data
- **ALWAYS review `notes/2025-06-17-data-driven-output-architecture.md` when working on `src/cfn/` command handlers**

---

## General requirements 
- Work to completion of your goal with 100% of tests passing, no regressions, and no new code warnings. 
- Don't stop to brag or celebrate. Keep going until you have completely reached the goal and completed all tasks.
- Use your Write tool to write files rather than echo or cat.
- 96% or 98% or even 99.6% tests passing is not completion of the goal. 100% is, but without reward hacks. 
- Do not claim that failing tests are edge cases or not important. That is for the user to determine.
- Do not create duplicate code.
- Use the correct existing constructors rather than creating new ones.

## Coding standards
- Act like a staff/principal engineer not a juniour or
  intermediate. Think and plan first. Review your changes critically
  as you go.
- use meaningful variable and fn names and omit useless comments. If a
  fn's purpose is clear there is no need for comment above it unless
  we are documenting it for the public api.
- comment only the non-obvious
- keep public APIs small. Do not bloat them or re-export what doesn't need exporting.
- Always import deps at the module level (use ...) at the top of the
  file. Do not import locally inside of fns or refer to types using
  the long 'crate::foo::Bar' / 'dep_crate::baz::Foo' syntax. That
  clutters the code.

## Testing
- run `cargo check --all` for a fast sanity check
- **All tests**: `cargo nextest r --color=never --hide-progress-bar`
- **Snapshot testing**: All example templates in `example-templates/` are automatically tested using `insta`
- Run tests: `cargo test --test example_templates_snapshots`
- Only the user may accept snapshot changes unless they explicitly tell you to and if valid: `cargo insta accept`, but only if the change is value and not a regression.
- Rather than creating adhoc rust binaries or tests not in tests/, just use the existing test infrastructure.
- Do not reward hack by commenting out tests or fudging to make them
  pass. Our goal is working software not tests that pretend to pass.

## Git Commit Requirements
- **Green commits only**: All tests must pass (100%) before committing.
- **No compiler warnings**: Fix all 'cargo check --all' warnings before committing
- **User review requried**: prior to commit, the user wants to review
  the changes and commit msg. When you are ready to commit, print a formatted commit msg to the user following the instructions below.
- **Accurate commit summaries**: The first line of the commit message must accurately reflect the full scope of changes
  - Don't list just 2 items if 5+ things were changed
  - Lead with the most important/impactful changes
  - Make the summary line broad enough to encompass all significant changes
  - Be specific about what was fixed/added/refactored
  - Do not mention things like 'all tests passing', 'no cargo check errors'. That is assumed.
  - Do not claim things like 'production ready' or '98%
    complete'. Keep it factual without judgements or claims.

## Development Commands
- Use the standard cargo stuff. 
- Do not use `rustc` directly. Use cargo.
- Use our local ./tmp/ dir instead of the system level /tmp
- Never `git checkout HEAD -- <file>`, `git reset`, or `git restore` without making
  a backup of the uncommitted changes and asking for user confirmation.

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

## YAML Tag Notes
- Yaml !Tags can't be nested directly like !Foo !Bar. You must instead do
!Foo
  - !Bar

## File Cleanup
- When I ask you to clean up temp or .bak files always read my instructions carefully and only remove the exact pattern I specifiy. If I say *.rs.bak, don't delete *.bak!
