# CLAUDE.md
This is a Rust rewrite of `iidy` https://github.com/unbounce/iidy, a CloudFormation deployment tool. 

## CURRENT WORK CONTEXT
n/a

### **Architecture References**
- **`notes/2025-06-17-data-driven-output-architecture.md`** - Core TUI architecture design
- **ALWAYS review `notes/2025-06-17-data-driven-output-architecture.md` when working on `src/cfn/` command handlers**

---

## General requirements and workflow.
- Act like a staff/principal engineer not a juniour or
  intermediate. 
  - Analyze, think, and plan first.
  - Consider edge cases, performance, and security.
- Each commit or larger series of commits, will have a timestamped plan
  file in the notes/ dir.
- Review our plan critically before starting. If the user is
  discussing the plan with you, ask for confirmation before switching
  into coding mode.
- Review our changes critically as you go. Document findings and
  progress in the plan file as you work.
- Work to completion of your goal with 100% of tests passing, no
  regressions, and no new code warnings.
- Don't stop to brag or celebrate. Keep going until you have
  completely reached the goal and completed all tasks.
- Use your Update/Write tool to write files rather than echo or cat.
- 96% or 98% or even 99.6% tests passing is not completion of the goal. 100% is, but without reward hacks. 
- DO NOT claim that failing tests are edge cases or not important. That is for the user to determine.
- DO NOT create duplicate code.
- Use the correct existing constructors rather than creating new ones.
- When removing code, do NOT leave a comment saying it was removed. We have git.
- DO NOT use emojis anywhere!

## Coding standards
- Use meaningful variable and fn names and DO NOT add useless comments. If a
  fn's purpose is clear there is no need for comment above it unless
  we are documenting it for the public api.
- Comment only the non-obvious.
- Keep public APIs small. Do not bloat them or re-export what doesn't need exporting.
- Always import deps at the module level (use ...) at the top of the
  file. Do not import locally inside of fns or refer to types using
  the long 'crate::foo::Bar' / 'dep_crate::baz::Foo' syntax. That
  clutters the code.
- All tests must be offline/deterministic using fixture data.

## Testing
- run `make check` for a fast sanity check
- `make test`
- All example templates in `example-templates/` are automatically tested as snapshots using `insta` (via `make test`)
- Only the user may accept snapshot changes unless they explicitly tell you to and if valid.
- DO NOT create adhoc rust binaries or tests not in tests/. Just use the existing test infrastructure.
- DO NOT reward hack by commenting out tests or fudging to make them
  pass. Our goal is working software not tests that pretend to pass.

## Development Commands
- make check, make test, make build
- DO NOT use `rustc` directly. Use cargo
- Use our local ./tmp/ dir instead of the system level /tmp
- Never `git checkout HEAD -- <file>`, `git reset`, or `git restore` without making
  a backup of the uncommitted changes and asking for user confirmation.
- Never create branches.

## Git Commit Requirements
- **Green commits only**: All tests must pass (100%) before committing.
- **No compiler warnings**: Fix all `make check` warnings before committing
- **User review requried**: The user will prompt you explicitly when it is time to commit.

## Project Documentation
See [notes/index.md](notes/index.md) for an overview of all design documents and implementation plans.

## Security
See [docs/SECURITY.md](docs/SECURITY.md) for comprehensive documentation on the YAML import system security model, including restrictions on remote template imports and base path derivation for relative imports.

## Architecture Overview

The project follows a modular structure:

### Core Components

- **CLI Layer** (`src/cli.rs`): Complete command-line interface using `clap`, supporting 20+ CloudFormation operations with AWS-specific options and environment-based configuration
- **AWS Integration** (`src/aws.rs`): AWS SDK configuration and credential management 
- **CloudFormation Operations** (`src/cfn/`): Individual modules for each CloudFormation operation (create, update, delete, describe, watch, etc.)
- **Template Preprocessing** (`src/yaml/`)
- **Output and formatting** (`src/output/`)

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
