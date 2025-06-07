# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands
- Use the standard rust / cargo stuff.
- prefer fd to find, rg to grep

## Testing
- **All tests**: `cargo test` or `make test`
- **Snapshot testing**: All example templates in `example-templates/` are automatically tested using `insta`
- Run tests: `cargo test --test example_templates_snapshots`
- Review snapshots: `cargo insta review` (requires `cargo install cargo-insta`)
- Accept changes: `cargo insta accept`
- Rather than creating adhoc rust binaries or tests not in tests/, just use the existing test infrastructure

## Coverage Reporting
- **Quick coverage**: `make coverage-quick`
- **HTML report**: `make coverage-html` (generates `tarpaulin-report.html`)
- **CI coverage**: `make coverage-ci` (70% threshold)
- **Full documentation**: See [docs/COVERAGE.md](docs/COVERAGE.md)

## Token Management
See [notes/2025-05-05-token-management-design.md](notes/2025-05-05-token-management-design.md) for comprehensive documentation on the client request token management system, including architecture, multi-step operations, and testing strategy.

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
- **Template Preprocessing** (`src/preprocess.rs`): Placeholder for YAML preprocessing system (currently stub, will implement iidy-js preprocessing language)

### Key Design Patterns

- **Async/Tokio Runtime**: All AWS operations use async/await with Tokio runtime created in `main.rs:31`
- **Offline testing**: The code is architected so it can be tested offline without connection to AWS:
  - AWS api operations are separate from output formatting and console IO code
  - Test fixtures are used to drive the latter.
- **Error Handling**: Uses `anyhow` for error propagation throughout the codebase
- **Environment-based Configuration**: Global `--environment` flag loads AWS profiles, regions, and other settings
- **Clap Command Structure**: Extensive use of derive macros for CLI with custom styling and shell completion support

### CloudFormation Operations

The `src/cfn/` modules implement AWS CloudFormation operations:
- Stack lifecycle: create, update, delete, create-or-update
- Change sets: create and execute changesets
- Monitoring: watch stack progress, describe drift
- Utilities: estimate costs, get templates, list instances

Most operations are currently stubs (`todo!()`) - implementations needed.

### Configuration Files

- **stack-args.yaml**: Primary configuration file for CloudFormation stacks, supporting parameters, tags, capabilities, IAM roles, and other CloudFormation options
- **Cargo.toml**: Uses AWS SDK v1, clap v4 for CLI, serde for YAML parsing, and tokio for async runtime

### Shell Completion

Generate completions via: `cargo run -- completion <shell>`

## YAML Preprocessing Language Porting Notes

Based on the upstream iidy documentation and implementation, the YAML preprocessing system needs to be ported from TypeScript to Rust.

### Core Preprocessing Features to Implement

#### Custom Tag System
- **Base Tag Architecture**: Generic `Tag` trait/struct for scalar, mapping, and sequence YAML nodes
- **Tag Registration**: System to register custom tags (`addTagType()` equivalent)
- **Runtime Type Safety**: Type-safe tag creation and manipulation

#### Preprocessing Language Tags
- **Data Import/Definition**:
  - `$imports`: Import data from external files/sources
  - `$defs`: Define local variables within document
- **Logical Operations**:
  - `!$if`: Conditional branching
  - `!$eq`: Equality comparison
  - `!$not`: Boolean negation
- **Data Transformation**:
  - `!$map`: Transform lists/arrays
  - `!$merge`: Combine mappings
  - `!$concat`: Merge sequences
  - `!$split`: String to array conversion
  - `!$join`: Array to string conversion
  - `!$let`: Local variable binding

#### String Processing (Handlebars-style)
- Template variable substitution: `{{variable}}`
- String helpers: `toLowerCase`, `toUpperCase`, `base64`
- Data conversion: `toJson`, `toYaml`

### Implementation Strategy for Rust

#### YAML Parser Integration Options

**Option 1: serde_yml (Recommended)**
- More advanced than `serde_yaml` with better custom tag support
- Native support for `!tag` syntax and enum serialization
- Provides `singleton_map` modules for flexible tag handling
- Better suited for iidy's custom tag requirements

**Option 2: yaml-rust**
- Lower-level YAML parsing with direct AST access
- More control over parsing pipeline but requires more manual work
- Currently limited custom tag support (listed as future goal)
- Would require building custom tag processing layer

**Option 3: serde_yaml + custom preprocessing**
- Current approach, extend existing `serde_yaml` usage
- More work to implement custom tag system
- May hit limitations in tag flexibility

**Recommended Approach: serde_yml**
- Replace current `serde_yaml` dependency with `serde_yml`
- Leverage its enhanced tag support for custom preprocessing tags
- Implement preprocessing pipeline using `serde_yml::Value` manipulation
- Use enum-based approach for different tag types

#### Tag Processing Pipeline
1. **Parse Phase**: Load YAML with custom schema recognition
2. **Preprocessing Phase**: Process all `!$` tags and `$` keys
3. **Resolution Phase**: Resolve imports, apply transformations
4. **Output Phase**: Generate final processed YAML/JSON

#### Rust-Specific Considerations
- Use `serde` traits for type-safe deserialization after preprocessing
- Use `anyhow` for preprocessing error chain propagation

#### Key Modules to Create
- `src/yaml/mod.rs`: Main YAML preprocessing entry point
- `src/yaml/tags.rs`: Custom tag implementations
- `src/yaml/imports.rs`: Import resolution system
- `src/yaml/handlebars.rs`: Template variable processing
- `src/yaml/transforms.rs`: Data transformation operations

This preprocessing system is essential for iidy's template composition and dynamic configuration capabilities.

## YAML Tag Notes
- Yaml !Tags can't be nested directly like !Foo !Bar. You must instead do
!Foo
  - !Bar