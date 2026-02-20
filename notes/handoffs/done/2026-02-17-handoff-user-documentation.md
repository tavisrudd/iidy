# Handoff: User-Facing Documentation

**Date**: 2026-02-17
**For**: Next Claude instance creating user-facing documentation
**Status**: Complete (2026-02-18)

## Completion Report

Executed 2026-02-18. All 5 documents created, reviewed against source code,
and cross-link verified.

| File | Action | Lines |
|------|--------|-------|
| `docs/import-types.md` | New | 257 |
| `docs/command-reference.md` | New | 601 |
| `docs/yaml-preprocessing.md` | Rewrite | 580 |
| `docs/getting-started.md` | New | 282 |
| `README.md` | Replace | 100 |
| `docs/README.md` | Update | 11 |
| `notes/index.md` | Update | +3 lines |

**Process**: Sonnet sub-agents drafted import-types.md and command-reference.md
in parallel. yaml-preprocessing.md, getting-started.md, and README.md written
directly. A Sonnet review sub-agent verified all 5 docs against source code
(cli.rs, stack_args.rs, engine.rs, all import loaders). Found and fixed 3 issues:
(1) `!$escape` description was inaccurate for the known buggy case -- rewritten
to document only the working behavior; (2) `cfn:stack` incorrectly listed as
unimplemented -- removed from the list; (3) `RoleARN` described as alias rather
than fallback.

**Execution notes**: `notes/handoffs/2026-02-18-user-documentation-execution.md`

---

## Goal

iidy has no user-facing documentation beyond the `--help` output and
the original iidy-js README. Create documentation that a CloudFormation
practitioner can use to adopt iidy without reading the source code.

## Approach

Use the sub-agent multi-round draft/review/edit pattern. For each document:
1. Draft with a Sonnet sub-agent (fast iteration)
2. Review with a second sub-agent for accuracy and completeness
3. Edit based on review
4. Test understanding: a third sub-agent reads the doc cold and tries to
   perform the described task. If it can't, the doc has gaps.
5. Final edit

**Style reference**: `../ssh-agent-guard/docs/` -- especially
`policy-guide.md` (task-oriented, concrete examples, progressive
disclosure), `system-setup.md` (setup instructions that explain why
each step matters), and `threat-model.md` (honest about limitations).

**Key principles**:
- No emojis
- No filler ("Welcome to iidy!" / "In this guide we'll...")
- Start with what the reader needs to do, not what iidy is
- Show real YAML, not abstract descriptions
- Every example must be copy-pasteable and correct
- Explain the "why" when the behavior isn't obvious

---

## Information Sources

The documentation must be derived from actual code behavior, not guesses.
Key sources:

- **CLI help**: `cargo run -- --help` and `cargo run -- <subcommand> --help`
  for every subcommand. This is the ground truth for flags and options.
- **src/cli.rs**: Clap derive structs with all option definitions, defaults,
  and descriptions.
- **example-templates/**: Working examples that are snapshot-tested. These
  are guaranteed correct.
- **tests/**: Integration tests show real usage patterns.
- **../iidy-js/README.md**: The original user documentation. Much of it
  applies to the Rust version but some features differ.
- **notes/codebase-guide.md**: Lists all preprocessing tags, import types,
  and CloudFormation operations.
- **notes/2026-02-17-project-review-and-next-steps.md**: Lists behavioral
  differences from iidy-js and what's not yet implemented.

---

## Document Plan

### 1. README.md (project root)

The current README (if any) needs replacement. Structure:

```
# iidy -- CloudFormation made tolerable

One-paragraph description: what iidy does and who it's for.

## Quick start
  Install, create a minimal stack-args.yaml, deploy a stack.
  (The entire flow in <20 lines of shell + YAML)

## What iidy does
  - YAML preprocessing (imports, variables, transforms)
  - CloudFormation lifecycle management
  - Interactive progress display

## Commands
  Table of all commands with one-line descriptions.
  Group by category: deploy, monitor, utility.

## Documentation
  Links to docs/ directory.

## Differences from iidy-js
  Brief summary, link to docs/js-compatibility.md for details.
```

**Verification**: Run every command shown in the README with `--help` to
confirm flag names and syntax are correct.

### 2. docs/getting-started.md

Step-by-step guide for a new user. Structure:

```
# Getting started

## Prerequisites
  - Rust toolchain (or prebuilt binary when available)
  - AWS credentials configured
  - A CloudFormation template

## Installation
  cargo install or nix build instructions

## Your first deployment
  1. Create stack-args.yaml (explain every field)
  2. Create a simple CFN template
  3. Run `iidy create-stack stack-args.yaml`
  4. Watch the output, explain what each section means
  5. Run `iidy describe-stack stack-args.yaml` to see the result
  6. Run `iidy delete-stack stack-args.yaml` to clean up

## Stack-args.yaml reference
  All fields with types and defaults.
  Environment map syntax.
  Template field (file path, S3 URL, render: prefix).

## Output modes
  Interactive (default), plain (--output-format plain), JSON (--output-format json)
  Keyboard controls (j/p/q/h)

## Next steps
  Link to preprocessing guide, command reference
```

**Source**: `src/cfn/stack_args.rs` for the StackArgs struct definition,
`src/cli.rs` for command-line options, `example-templates/` for working
examples.

### 3. docs/preprocessing-guide.md

The YAML preprocessing language explained for users. This is the most
important user doc because preprocessing is what makes iidy valuable.

```
# YAML preprocessing

## How it works
  Brief explanation of the two-phase pipeline from a user perspective.
  (Not the internal implementation -- the user mental model.)

## Variables and imports ($imports, $defs)
  How to define variables with $defs (with examples)
  How to import from files, environment, S3, SSM, CFN exports
  Import type reference table
  Handlebars interpolation in import paths

## Handlebars templates
  {{ variable }} syntax
  Available helpers (list all ~25 with examples)
  Escaping: \{{ to produce literal braces

## Preprocessing tags
  One section per tag with:
  - What it does (one sentence)
  - Syntax
  - Example (input YAML -> output YAML)
  - Edge cases / gotchas

  Group by purpose:
  - Lookup: !$ (variable lookup)
  - Control flow: !$if, !$let
  - Collections: !$map, !$concat, !$merge, !$concatMap, !$mergeMap,
    !$mapListToHash, !$mapValues, !$groupBy, !$fromPairs
  - Strings: !$join, !$split
  - Serialization: !$toYamlString, !$parseYaml, !$toJsonString, !$parseJson
  - Comparison: !$eq, !$not
  - Escaping: !$escape

## CloudFormation tag pass-through
  !Ref, !Sub, !GetAtt, etc. are recognized and preserved.
  Preprocessing happens inside CFN tags.

## Debugging
  `iidy render` to see preprocessed output
  Error messages: how to read them, common mistakes
```

**Verification**: For every example shown, create a test fixture in
`example-templates/` and verify it produces the expected output via
`cargo run -- render <file>`.

### 4. docs/command-reference.md

Complete reference for all commands. Generate from `src/cli.rs`.

For each command:
- Synopsis (command + required args + common flags)
- Description (what it does)
- Options table (flag, type, default, description)
- Example usage
- Exit codes

Group by category:
- Stack lifecycle: create-stack, update-stack, delete-stack, create-or-update
- Changesets: create-changeset, exec-changeset
- Monitoring: watch-stack, describe-stack, describe-stack-drift
- Information: list-stacks, get-stack-template, get-import
- Utilities: render, estimate-cost, explain
- Template approval: template-approval-request, template-approval-review

Note which commands are stubs (param, lint-template, convert-stack-to-iidy,
init-stack-args).

**Source**: `src/cli.rs` is the single source of truth. Cross-reference
with the command handler files in `src/cfn/` for behavior details.

### 5. docs/import-types.md

Reference for all import source types. Structure:

```
# Import types

## file (default)
  file:path or just path
  Relative path resolution rules
  ?optional prefix for missing files

## env
  Environment variable lookup
  Type coercion (number, boolean, JSON)

## s3
  S3 URL format, authentication
  Auto-signing for large templates

## http / https
  Remote template fetching
  Security restrictions (remote templates cannot import local files)

## ssm / ssm-path
  SSM Parameter Store lookup
  Path prefix enumeration

## cfn
  CloudFormation stack outputs and exports
  cfn:stack.Output, cfn:export:name

## git
  Git metadata (branch, commit, etc.)

## random
  Random value generation

## filehash / filehash-base64
  File content hashing
```

**Source**: `src/yaml/imports/loaders/` -- one file per import type.
`docs/SECURITY.md` for the security model.

---

## What NOT to Document

- Internal architecture (that goes in developer docs, not user docs)
- Features that don't exist yet (custom resource templates, `!$expand`)
- Stub commands (mention they exist but are not yet implemented)
- The iidy-js version (this is documentation for the Rust version)

## Quality Checklist

For every document:
- [ ] No emojis
- [ ] No filler text
- [ ] Every YAML example is valid and tested
- [ ] Every command example uses correct flags (verified against `--help`)
- [ ] File paths and import URIs are realistic, not `foo.yaml`
- [ ] Error scenarios are documented (what happens when X goes wrong)
- [ ] Cross-links between docs use relative paths
- [ ] A CloudFormation practitioner who has never used iidy can follow the
      getting-started guide without referring to source code

---

## Prose and Structure Recommendations

Notes from reviewing the iidy-js docs (`../iidy-js/README.md` and
`../iidy-js/docs/`) and the ssh-agent-guard docs
(`../ssh-agent-guard/docs/`). The iidy-js docs are functional but have
specific weaknesses to avoid. The ssh-agent-guard docs are the quality
target.

### What the iidy-js docs get wrong

**The README tries to be everything.** It's a getting-started guide, a
reference, and an API doc in one file. The args-file properties table is
useful but sits alongside installation instructions and development notes.
Split these into separate docs. The README should be a landing page with
links, not a monolith.

**The preprocessing doc is a flat list of examples with minimal
explanation.** Each tag gets a code block but no prose explaining when
you'd use it, what the mental model is, or how it composes with other
tags. A user can see that `!$map` maps a template over items, but
doesn't learn:
- When to use `!$map` vs `!$mergeMap` vs `!$concatMap`
- How `var:` scoping works (the default `item` vs custom names)
- That `filter:` is evaluated per-item with the loop variable in scope
- That tags compose (you can nest `!$map` inside `!$concatMap`)

**No progressive disclosure.** The docs go straight from "here's
`$defs`" to `!$concatMap` with nested `!$map` and no intermediate
steps. A reader who just wants to parameterize a template file has
to wade through advanced collection transforms to find `$imports`
and `!$`.

**The custom resource templates doc is unfinished.** It has one example
and two TODO markers. This is the most powerful feature and it has the
least documentation.

### Exemplar documentation to model after

Study these before drafting. Each maps to a specific iidy doc:

| iidy doc | Model after | Why |
|----------|------------|-----|
| README.md | ripgrep README | Short landing page, links to guide, not a monolith |
| getting-started.md | Jsonnet tutorial | Progressive disclosure: plain file, add a variable, add an import, add a function |
| preprocessing-guide.md | jq manual | Input/output examples, composability explanations, "when to use X vs Y" |
| command-reference.md | gh CLI manual | Synopsis + examples + flags table per command, grouped by category |
| import-types.md | Terraform provider docs | One section per type, consistent structure |

**jq manual** (https://jqlang.github.io/jq/manual/) is the single most
important exemplar. It documents each filter with: one-line description,
syntax, example with input/output side by side, and how filters compose.
iidy's `!$map`/`!$concat`/`!$merge` family has the same "small composable
transforms" design. If the preprocessing guide reaches jq manual quality,
the rest follows naturally.

**Jsonnet tutorial** (https://jsonnet.org/learning/tutorial.html) --
another data templating language for generating config (including
CloudFormation). Their tutorial progression from "here's plain JSON" to
"now add a variable, now a function, now imports" is exactly the arc
the getting-started guide should follow.

**ripgrep GUIDE.md** (BurntSushi/ripgrep repo) -- walks through real
scenarios with the thought process visible, not just flag listings. The
command reference is separate from the guide.

**gh CLI manual** (https://cli.github.com/manual/) -- good command
reference structure. Each command: synopsis, description, examples,
flags, "see also". Grouping by category (pr, issue, repo) maps to
iidy's grouping (stack lifecycle, changesets, monitoring, utilities).

**Terraform docs** (https://developer.hashicorp.com/terraform/docs) --
same IaC domain. Their three-tier split (tutorials / documentation /
reference) is the right overall structure.

For prose style within pages, the ssh-agent-guard docs
(`../ssh-agent-guard/docs/`) remain the quality target: direct,
concrete, no hedging, explains "why" alongside "what".

### Structure recommendations for each doc

#### README.md

Open with what the tool does in one sentence, not a bullet list of
features. The iidy-js README opens with 8 bullet points before showing
any usage. Instead:

```
iidy deploys CloudFormation stacks with a YAML preprocessing layer
that supports imports, variables, conditionals, and collection
transforms. It wraps the CloudFormation API with readable progress
output and changeset support.
```

Then a 10-line "deploy a stack in 60 seconds" example that a reader can
copy-paste. The iidy-js README has no quick-start -- it goes straight to
`iidy help` output, which is a wall of text.

After the quick start: a brief "what iidy adds over raw CloudFormation"
section (3-4 bullets max), then links to the real docs.

#### docs/getting-started.md

Structure as a tutorial, not a reference. Walk through one complete
scenario: create a stack, watch it, describe it, update it, delete it.
Each step shows the command AND the output (abbreviated).

The stack-args.yaml reference table from iidy-js README is good content
but belongs in a reference section within this doc or in the command
reference, not at the top.

Key improvement over iidy-js: explain the `render:` prefix on Template.
The iidy-js docs mention it in passing (`"render:./cfn-template.yaml"`)
but never explain what it means or when you need it. This is the single
most common point of confusion for new users: "when does preprocessing
happen on my template?"

Answer: always on stack-args.yaml, only on the CFN template if you use
`render:` prefix.

#### docs/preprocessing-guide.md

This is the most important doc. Structure it in three tiers:

**Tier 1: The basics** (covers 80% of usage)
- `$imports` and `$defs` -- defining and using variables
- `!$` include tag -- splicing values into output
- `{{ }}` handlebars -- interpolating values into strings
- `!$if` / `!$eq` / `!$not` -- conditionals
- `!$let` -- local bindings
- `render:` prefix and `iidy render` command

This tier should be self-contained. A reader who stops here can be
productive.

**Tier 2: Collections** (covers the next 15%)
- `!$map` with a full explanation of how template/items/var/filter work
- `!$concat` and `!$merge` -- combining lists and maps
- `!$concatMap` and `!$mergeMap` -- map then combine
- `!$fromPairs` and `!$mapListToHash` -- restructuring data
- `!$mapValues` and `!$groupBy`

Each tag should have:
1. One-sentence description
2. Syntax summary
3. Minimal example (input -> output)
4. A "when to use this" sentence distinguishing it from similar tags
5. One realistic example showing actual CloudFormation usage

The "when to use this" part is what the iidy-js docs completely lack.
For example: "Use `!$mergeMap` when you need to produce a mapping (not
a list) from a list of inputs. Common for generating CloudFormation
`Resources` blocks where each resource has a unique logical name."

**Tier 3: Serialization and advanced** (5%)
- `!$toYamlString` / `!$parseYaml` / `!$toJsonString` / `!$parseJson`
- `!$split` / `!$join`
- `!$escape`
- Import types reference (detailed, with all subtypes)
- Handlebars helpers reference table

#### docs/command-reference.md

Generate the synopsis from `cargo run -- <cmd> --help` output, but
don't just paste it. Add:
- A one-paragraph description of what the command does and when to use it
- The most common invocation (not every flag)
- What "success" looks like (exit code 0, what output to expect)
- What "failure" looks like (common errors and what they mean)

The iidy-js README just pastes the full help output. That's useful as a
quick reference but useless for understanding what a command does.

### Tone and voice

Match the ssh-agent-guard docs: direct, concrete, no hedging. Write
as if explaining to a colleague, not selling to a customer.

Bad: "iidy provides a powerful and flexible YAML preprocessing system
that enables users to..."

Good: "iidy preprocesses YAML files before passing them to
CloudFormation. `$imports` loads external data. `$defs` defines local
variables. `!$` splices values into the output."

Bad: "You may optionally choose to use the `!$map` tag when you need
to iterate over a collection."

Good: "`!$map` applies a template to each item in a list."

### Concrete material to reuse

The `example-templates/yaml-iidy-syntax/` directory has one file per
tag, all snapshot-tested. These are the canonical examples. Use them
as the basis for documentation examples rather than inventing new ones.

The iidy-js `docs/yaml-preprocessing.md` examples are mostly correct
and can be adapted, but verify each against the Rust implementation's
behavior since there are known differences (e.g., `$defs` has let*
semantics in Rust vs parallel in JS; `!$string` alias doesn't exist
in Rust yet; `!$escape` behavior differs).

### Handlebars helpers to document

The Rust version registers 27 helpers (including deprecated aliases):

| Helper | Category | Deprecated alias |
|--------|----------|-----------------|
| toJson | Serialization | tojson |
| toJsonPretty | Serialization | tojsonPretty |
| toYaml | Serialization | toyaml |
| base64 | Encoding | -- |
| urlEncode | Encoding | -- |
| sha256 | Encoding | -- |
| filehash | Encoding | -- |
| filehashBase64 | Encoding | -- |
| toLowerCase | String case | -- |
| toUpperCase | String case | -- |
| titleize | String case | -- |
| camelCase | String case | -- |
| pascalCase | String case | -- |
| snakeCase | String case | -- |
| kebabCase | String case | -- |
| capitalize | String case | -- |
| trim | String manipulation | -- |
| replace | String manipulation | -- |
| substring | String manipulation | -- |
| length | String manipulation | -- |
| pad | String manipulation | -- |
| concat | String manipulation | -- |
| lookup | Object access | -- |

Note: iidy-js includes the full `handlebars-helpers` string category
(~30 helpers). The Rust version has a curated subset. The docs should
list exactly what's available, not reference the npm package.

### Import types: what Rust supports vs iidy-js

Document only what works. The Rust version supports:
- file, env, git, random, filehash, filehash-base64, s3, http/https,
  cfn (stack.Output and export only), ssm, ssm-path

It does NOT support (document as "not yet implemented"):
- cfn:parameter, cfn:tag, cfn:resource, cfn:stack
- ?region= query parameter on cfn imports

### Known behavioral differences to call out

These trip up users migrating from iidy-js:
1. `$defs` resolves sequentially (let* semantics) -- each def can
   reference prior defs. iidy-js resolves in parallel.
2. `!$string` alias for `!$toYamlString` does not exist.
3. `iidy.*` implicit variables (`iidy.region`, `iidy.environment`,
   etc.) are not injected.
4. `render` command only accepts a single file, not a directory.
5. Custom resource templates (`$params`, `!$expand`) not yet implemented.
