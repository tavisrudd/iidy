# Project Review and Next Steps

**Date**: 2026-02-17
**Context**: Resuming work after ~7 month pause. Last activity was Oct 2025.

## Current State

The project is in strong shape. 608 tests pass, zero compiler warnings, and
the codebase is clean.

### What's Done

**YAML preprocessing engine** -- substantially complete:
- 20 preprocessing tags, all CloudFormation intrinsic function pass-through
- Full import system (file, env, git, random, filehash, s3, http, cfn, ssm, ssm-path)
- Handlebars interpolation with ~25 custom helpers
- Tree-sitter-based parser with location tracking, enhanced error reporting
- YAML 1.1/1.2 compatibility handling
- Extensive test coverage: property tests, snapshot tests, equivalence tests, 100+ snapshots

**CloudFormation operations** -- all major commands implemented:
- Stack lifecycle: create, update, delete, create-or-update
- Changesets: create, execute
- Monitoring: watch, describe, drift detection
- Utilities: list, get-template, get-instances, estimate-cost, get-import
- Template approval workflow (request + review)
- Stack-args loading with environment-map resolution, AWS config merging

**Output system** -- data-driven architecture with 25 OutputData variants:
- Interactive renderer (with spinners, ANSI, themes)
- JSON renderer (JSONL format)
- Plain mode (interactive renderer with features disabled)
- Keyboard-driven mode switching, confirmation prompts

**Other**: demo command, explain command, shell completion, NTP timing,
idempotency token management.

### What's Stubbed (println! only)

- `param` (set, review, get, get-by-path, get-history) -- SSM CRUD
- `lint-template` -- delegated to `laundry-cfn` in JS; needs a strategy
- `convert-stack-to-iidy` -- reverse-engineers live stack to iidy project
- `init-stack-args` -- scaffolds new project files

---

## Primary Next Step: Custom Resource Templates

**Note**: `example-templates/custom-resource-templates/` is excluded from
snapshot auto-discovery (in `tests/example_templates_snapshots.rs`) until
this feature is implemented. Remove the exclusion when done.

This is the most significant missing feature in the YAML preprocessing engine.
It's the module/component system for CloudFormation: define reusable templates
with `$params` that expand into sets of real AWS resources.

### How It Works in iidy-js

**Template definition** -- a YAML file with `$params` and CFN sections:

```yaml
# my-template.yaml
$params:
  - Name: Foo
    Type: string
  - Name: Bar
    Type: string
    Default: "default-bar"
Resources:
  Topic:
    Type: AWS::SNS::Topic
    Properties:
      DisplayName: "{{Foo}}-{{Bar}}"
Outputs:
  TopicArn:
    Value: !GetAtt Topic.Arn
```

**Usage as a synthetic resource type** in a CFN template:

```yaml
$imports:
  MyTemplate: my-template.yaml
Resources:
  Web:
    Type: MyTemplate
    Properties:       # maps to $params
      Foo: web-service
  Api:
    Type: MyTemplate
    Properties:
      Foo: api-service
      Bar: custom-bar
```

**Preprocessed output** -- each usage expands with name-prefixing:

```yaml
Resources:
  WebTopic:
    Type: AWS::SNS::Topic
    Properties:
      DisplayName: "web-service-default-bar"
  ApiTopic:
    Type: AWS::SNS::Topic
    Properties:
      DisplayName: "api-service-custom-bar"
Outputs:
  WebTopicArn:
    Value: !GetAtt WebTopic.Arn
  ApiTopicArn:
    Value: !GetAtt ApiTopic.Arn
```

### Key Mechanisms to Implement

**1. `$params` declaration and validation** (`index.ts:92-99, 551-642`)

Templates declare parameters as a `$params` array:
```
$param = { Name, Default?, Type?, Schema?, AllowedValues?, AllowedPattern? }
```

Validation supports: JSON Schema (via tv4), AllowedValues, AllowedPattern,
and basic type checking (string, number, object, CFN parameter types).

Name clash detection: `$params` names must not collide with `$imports`/`$defs`.

**2. Custom resource type detection** (`visitor.ts:686-710, 733-744`)

When visiting the `Resources` section at `Root.Resources`, the visitor checks
each resource's `Type`. If the type name exists in `$envValues` (i.e., it was
imported and has `$params`), it's a custom resource template. Otherwise it must
start with `AWS` or `Custom` (standard CFN types).

**3. Template expansion with name-prefixing** (`visitor.ts:747-827`)

For each custom resource usage:
- `Prefix` defaults to the resource's logical name (e.g., `Web`), overridable
  via `NamePrefix` property
- `Properties` on the call site map to the template's `$params`
- Params are merged: `$param` defaults (resolved in template's env) then
  caller-provided values on top
- `Overrides` deep-merges into the template before expansion (evaluated in
  caller's env, not template's)
- Template's `Resources` are visited in a sub-environment with `Prefix` set
- Output resource names are prefixed: `Topic` becomes `WebTopic`

**4. Ref/GetAtt/Sub rewriting** (`visitor.ts:456-547`)

Inside a custom resource expansion (when `Prefix` is set):
- `!Ref Foo` becomes `!Ref WebFoo`
- `!GetAtt Foo.Arn` becomes `!GetAtt WebFoo.Arn`
- `!Sub "${Foo}"` becomes `!Sub "${WebFoo}"`
- `Condition:` and `DependsOn:` values on resources are also rewritten

Exempt from rewriting:
- Names starting with `AWS:` (pseudo-parameters like `AWS::Region`)
- Names in the `$globalRefs` set (see below)

**5. Global section promotion** (`visitor.ts:829-853`, `index.ts:688-699`)

Templates can define `Parameters`, `Conditions`, `Mappings`, `Outputs`,
`Metadata`, `Transform` sections. These are "promoted" to the root CFN
document via `GlobalAccumulator` (a shared mutable accumulator passed through
the `Env`). Names are prefixed unless marked `$global`.

The accumulator is merged into the final output after all Resources are
visited.

**6. `$global` flag** (`visitor.ts:767-776, 818-826, 839-847`)

Any item in a template's Resources or global sections can set `$global: true`
to suppress name-prefixing. This is used for shared/singleton resources that
should not be duplicated when multiple instances of a template are used.

The `$global` property is stripped from the output after processing.
Resources marked `$global` are also added to `$globalRefs` so that
`!Ref`/`!GetAtt`/`!Sub` references to them are not rewritten.

**7. `!$expand` tag** (`visitor.ts:182-208`)

A separate, simpler mechanism for non-CFN template expansion:
```yaml
result: !$expand
  template: MyImportedTemplate
  params:
    Foo: 123
```

Unlike custom resource types, `!$expand` does NOT set a Prefix, does NOT
rewrite refs, and does NOT promote global sections. It just expands the
template inline with params. This is useful outside the `Resources` section.

### Implementation Approach

The Rust YAML engine currently has no concept of `$params`, `Prefix`,
`$globalRefs`, `GlobalAccumulator`, or the `!$expand` tag.

Suggested order:

1. **`$params` parsing and validation** -- extend the AST and parser to
   recognize `$params` in documents. Implement `validateTemplateParameter`.
   This is foundational.

2. **`!$expand` tag** -- simpler than custom resource types (no prefixing).
   Good incremental step to get template expansion working.

3. **Custom resource type detection in `Resources`** -- the resolver needs
   special handling for `Root.Resources` that checks each resource's `Type`
   against the environment.

4. **Name-prefixing and ref rewriting** -- `Prefix` in the resolution
   context, rewriting `!Ref`, `!GetAtt`, `!Sub`, `Condition`, `DependsOn`.

5. **`GlobalAccumulator` and section promotion** -- accumulate and merge
   template sections into the root document.

6. **`$global` flag** -- suppress prefixing for marked items.

7. **`Overrides` and `NamePrefix`** -- deep-merge and prefix override.

Real-world examples from the user will be essential for designing tests and
catching edge cases, since the iidy-js test suite has almost no end-to-end
coverage of this feature (all `$params` tests are skipped stubs).

---

## Secondary Items

### Bugs / Correctness Issues

**`!$groupBy` non-deterministic output** (`resolver.rs`): Uses `HashMap` for
grouping, so output key order varies between runs. Fix: collect into Vec, sort
by key, then insert into the output Mapping.

**`!$escape` on preprocessing tags** (`resolver.rs:629`): Produces hardcoded
`"!$escaped_tag"` string instead of preserving the tag. The JS version returns
the raw parsed data.

**`$envValues` not implemented** (`engine.rs`, `resolver.rs`): Recognized only
as a key to suppress from output. In JS, it's a first-class document field that
seeds the preprocessing environment, and `iidy.*` namespace variables
(`iidy.command`, `iidy.environment`, `iidy.region`, `iidy.profile`) are
injected into it by the runtime. This matters for the custom resource template
feature since templates carry their own `$envValues`.

**`$defs` has let* semantics, JS has parallel**: Rust resolves each def
sequentially (each can reference prior defs). JS copies defs raw into
`$envValues` without resolving -- they're resolved later during visitation.
This is a behavioral difference: cross-referencing within a single `$defs`
block works in Rust but would fail in JS. Verify whether this divergence is
intentional or accidental.

**`!$string` alias missing**: JS registers `!$string` as an alias for
`!$toYamlString`. Rust only recognizes `!$toYamlString`.

**Error display panic risks** (`errors/wrapper.rs`): 18 `TODO: PANIC
POTENTIAL` markers, mostly around string slice operations with byte indices.
The genuine risks are multibyte UTF-8 content in template lines that pass
through error formatting. Low probability in practice (CFN templates are
typically ASCII) but would mask the original error.

**Handlebars registry recreated per string** (`handlebars/engine.rs`): The
registry with ~25 helpers is rebuilt on every interpolation call. Fix with
`OnceLock` or `Lazy`.

### Missing but Lower Priority

**`cfn:` import subtypes**: Only `cfn:stack.Output` and `cfn:export:` are
implemented. JS also supports `cfn:parameter:`, `cfn:tag:`, `cfn:resource:`,
`cfn:stack:`, and an optional `?region=` query parameter.

**`render` with directory input**: JS iterates `.yml`/`.yaml` files in a
directory. Rust only accepts a single file path.

**Stack-args file detection in `render`**: JS auto-detects stack-args files
(by filename or document structure) and routes them through the full
stack-args loading pipeline. Rust always uses the generic preprocess path.

**`iidy.*` variable injection**: JS injects `iidy.command`,
`iidy.environment`, `iidy.region`, `iidy.profile` into `$envValues` before
preprocessing. Rust does not.

**Output system TODOs**: `ToggleTimestamps` keyboard command is a stub.
`TokenInfo` is silently discarded in interactive mode. Help text in
`keyboard.rs` uses emojis (violates project standards).

### Stub Commands (low priority)

- `param` -- SSM CRUD with KMS alias auto-discovery, `.pending` approval flow
- `lint-template` -- JS delegates to `laundry-cfn` npm package
- `convert-stack-to-iidy` -- non-trivial reverse-engineering with context-aware
  deep-sort of CFN documents
- `init-stack-args` -- trivial file scaffolding
