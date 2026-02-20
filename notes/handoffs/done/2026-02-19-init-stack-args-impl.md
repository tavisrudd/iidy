# init-stack-args -- Implementation Plan

**Date**: 2026-02-19
**Session**: `da44b6f0-9f09-47fe-9b2a-66dc5088a8e3`
**References**: `../iidy-js/src/initStackArgs.ts`

## Context

`init-stack-args` is the simplest of the three remaining stubs. It scaffolds
two starter files (`stack-args.yaml` and `cfn-template.yaml`) into the current
directory. No AWS calls, no template processing, no YAML engine -- pure file I/O
with existence checks and force flags.

## Implementation

### What JS does

1. Check if `./stack-args.yaml` exists. If it does and no `--force` /
   `--force-stack-args`, print a message and skip. Otherwise write it.
2. Same for `./cfn-template.yaml` with `--force` / `--force-cfn-template`.
3. Always return exit code 0.

The generated `stack-args.yaml` is a ~60-line commented YAML scaffold showing
all available options (StackName, Template, Tags, Parameters, Capabilities,
NotificationARNs, RoleARN, TimeoutInMinutes, OnFailure, StackPolicy,
ResourceTypes, CommandsBefore). The generated `cfn-template.yaml` is a 3-line
minimal CFN template with a dummy `WaitConditionHandle` resource.

### Rust implementation

Create `src/cfn/init_stack_args.rs`:

```rust
use anyhow::Result;
use std::path::Path;

use crate::cli::InitStackArgs;

const STACK_ARGS_TEMPLATE: &str = r#"# ..."#; // ~60 lines, match JS
const CFN_TEMPLATE: &str = r#"Dummy:
    Type: "AWS::CloudFormation::WaitConditionHandle"
    Properties: {}"#;

pub fn init_stack_args(args: &InitStackArgs) -> Result<i32> {
    let force_stack_args = args.force || args.force_stack_args;
    let force_cfn_template = args.force || args.force_cfn_template;

    write_if_absent("stack-args.yaml", STACK_ARGS_TEMPLATE, force_stack_args);
    write_if_absent("cfn-template.yaml", CFN_TEMPLATE, force_cfn_template);
    Ok(0)
}

fn write_if_absent(filename: &str, content: &str, force: bool) {
    if Path::new(filename).exists() && !force {
        eprintln!("{filename} already exists! See help [-h] for overwrite options");
    } else {
        std::fs::write(filename, content).unwrap_or_else(|e| {
            eprintln!("Failed to write {filename}: {e}");
        });
        eprintln!("{filename} has been created!");
    }
}
```

Wire in `src/main.rs`:
```rust
Commands::InitStackArgs(args) => {
    return Ok(crate::cfn::init_stack_args::init_stack_args(&args)?);
}
```

Register in `src/cfn/mod.rs`.

### Template content

Copy the stack-args scaffold text from `iidy-js/src/initStackArgs.ts` verbatim
(the commented YAML block). It's documentation, not code -- match it exactly.

## Codebase Reference

| What | Where |
|------|-------|
| CLI struct | `src/cli.rs:677-684` |
| Main dispatch stub | `src/main.rs:233` |
| JS implementation | `../iidy-js/src/initStackArgs.ts` |
| cfn module registry | `src/cfn/mod.rs` |

## Build/Test Commands

Per CLAUDE.md. No special commands needed. This is a pure offline command
so a simple unit test can verify the file-writing logic using `tmp/` dir.

## Delegation Strategy

- **Can delegate?** Yes
- **Sub-agent type**: Sonnet (straightforward file I/O, no complex logic)
- **Why**: Isolated module, clear spec, no interactions with other systems

## Workflow Instructions

1. Read this file
2. Copy scaffold text from JS source
3. Implement `src/cfn/init_stack_args.rs`
4. Wire into main.rs and cfn/mod.rs
5. Add a unit test that writes to `tmp/` and verifies content
6. `make check-fast` + `make test`

## Progress

- [x] Implement init_stack_args.rs with scaffold templates
- [x] Wire into main.rs dispatch and cfn/mod.rs
- [x] Add unit tests (4 tests: create, skip-existing, force-all, force-individual)
- [x] make check-fast + make test pass (629 tests, 0 failures)
