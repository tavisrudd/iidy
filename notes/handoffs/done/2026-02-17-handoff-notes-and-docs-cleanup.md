# Handoff: Notes Cleanup and Developer Documentation

**Date**: 2026-02-17
**For**: Next Claude instance organizing project documentation

## Goal

The `notes/` directory has 50+ files accumulated over 8 months of
development. Most are timestamped work session logs that are useful as
historical record but clutter navigation. This handoff covers:

1. Archiving stale session notes
2. Updating the notes index
3. Creating permanent developer documentation in `docs/`

## Approach

Use sub-agents with the multi-round draft/review/edit pattern from
`../ssh-agent-guard/docs/`. For each document:
1. Sub-agent drafts the document (Sonnet for speed)
2. Second sub-agent reviews for accuracy, completeness, tone (Sonnet)
3. Apply review feedback, edit
4. Third sub-agent reads the final doc cold and tries to answer questions
   a developer would have -- if it can't, the doc has gaps
5. Final edit pass

The reference docs at `../ssh-agent-guard/docs/` set the quality bar:
prose-first, concrete, no filler, explains "why" alongside "what", uses
examples, cross-links related docs. Read `threat-model.md`,
`defense-in-depth.md`, `caller-identification.md` for style reference.

**Important**: Do not use emojis anywhere. Do not add filler text or
marketing language. Write for a staff engineer who needs to understand
the system quickly.

---

## Part 1: Archive Stale Notes

### What to archive

Move completed work-session notes to `notes/archive/`. These are valuable
as git history but should not clutter the active directory.

**Criteria for archiving**: The note documents work that is 100% complete
and has no open TODOs that are still relevant.

Likely candidates (verify each by reading the first ~20 lines):

```
notes/archive/   (create this directory)

# Completed implementation plans
2025-06-05-plan.md
2025-06-05-token-management-design.md
2025-06-06-error-reporting-improvement-design.md
2025-06-06-phase-1-core-yaml.md
2025-06-07-color-theming-design.md
2025-06-08-yaml-mod-code-review-by-opus.md
2025-06-09-scope-system-and-performance-optimization.md
2025-06-14-stackframe-removal-decision.md
2025-06-14-stackframe-removal-progress.md
2025-06-16-parser-multi-error-collection-design.md
2025-06-16-parser-multi-error-collection-design-orig.md
2025-06-16-parsing-test-cleanup.md
2025-06-16-use-src-meta-in-errors.md
2025-06-17-complete-iidy-implementation-spec.md
2025-06-17-console-output-modes.md
2025-06-17-data-driven-output-architecture-implementation.md
2025-06-17-path-tracking-parser-experiment.md
2025-06-18-aws-error-handling-notes.md
2025-06-18-context-window-recovery-instructions.md
2025-06-18-critical-stack-args-implementation-plan.md
2025-06-18-stack-args-loading-analysis.md
2025-06-20-list-stacks-json-query-implementation.md
2025-06-20-plain-renderer-race-condition-fix.md
2025-06-20-s3-url-auto-signing-implementation.md
2025-06-20-stack-events-title-configuration.md
2025-07-01-cfn-handler-normalization-plan.md
2025-07-02-confirmation-prompts-design.md
2025-07-04-changeset-expected-sections-fix.md
2025-07-04-changeset-operations-architectural-review.md
2025-07-05-demo-rs-completion-analysis.md
2025-07-05-get-import-command-analysis.md
2025-07-06-cfn-command-exit-code-consistency-analysis.md
2025-07-07-get-import-migration-analysis.md
2025-07-09-code-duplication-analysis.md
2025-07-09-code-duplication-bug-analysis.md
2025-09-27-defs-variable-resolution-bug-analysis.md
2025-09-28-renderer-cli-required-prop.md
2025-09-29-native-anchor-alias-support.md
2025-10-01-aws-config-duplication-analysis.md
2025-10-01-region-display-bug.md
2025-10-02-credential-source-display-plan.md
2025-10-02-remove-sts-calls-from-renderer.md
2025-10-03-demo-secret-masking.md
2025-01-16-comprehensive-test-coverage-plan.md
2025-01-16-multi-error-parser-review.md

# Codex plans (all completed)
codex/  (move entire directory)
```

### What to keep in notes/

```
# Active/reference documents
codebase-guide.md                              -- keep, update
index.md                                       -- keep, rewrite
2026-02-17-project-review-and-next-steps.md    -- active
2026-02-17-custom-resource-templates-rfc.md    -- active RFC
2026-02-17-code-review-findings.md             -- active
2026-02-17-handoff-code-review.md              -- active handoff
2026-02-17-handoff-code-review-fixes.md        -- active handoff
this file                                      -- active handoff

# Promote to docs/ (see Part 3)
2025-06-17-data-driven-output-architecture.md  -- becomes docs/dev/output-architecture.md
ADR-2025-07-06-output-sequencing-architecture.md -- becomes docs/dev/adr/001-output-sequencing.md
template-approval-design-spec.md               -- becomes docs/dev/adr/003-template-approval.md
aws-config-resolution-order.md                 -- becomes docs/dev/aws-config.md
BENCHMARKS.md                                  -- becomes docs/benchmarks.md
IIDY_JS_COMPATIBILITY_FIXES.md                 -- becomes docs/dev/js-compatibility.md
aws_api_analysis.org                           -- becomes docs/aws-api-analysis.md (convert from org)
```

### Update index.md

Rewrite `notes/index.md` to reflect the new structure. It should be a
short document:
- Link to `docs/` for permanent documentation
- Link to `notes/archive/` for historical session logs
- List only the active notes with one-line descriptions

---

## Part 2: Audit Stale Information

Before archiving, check these documents for information that contradicts
current reality:

### codebase-guide.md

- Verify all file paths still exist (use `ls` or `stat`)
- Verify line counts are still approximately correct
- Update any section that references features as "not implemented" if
  they have been implemented
- The "Behavioral Differences from Rust Version" section needs review --
  some differences may have been resolved

### CLAUDE.md (project root)

- The "CURRENT WORK CONTEXT" section says "n/a" -- this is correct for
  now but should be updated when work begins
- Verify all `make` commands still work
- Verify the `notes/index.md` reference still makes sense after rewrite

### docs/SECURITY.md (user-facing, at `docs/SECURITY.md`)

- Contains emoji in section headers (line 25: "Local-Only Import Types").
  Fix during the doc rewrite.
- Verify the security model description matches current code behavior

### docs/dev/COVERAGE.md

- References `cargo-tarpaulin` and `make coverage-*` targets. Verify
  these still exist in the Makefile. If not, either add them or update
  the doc.

---

## Part 3: Create Permanent Developer Documentation

Create a proper `docs/` directory structure. User-facing docs live at `docs/`,
developer/agent-internal docs live at `docs/dev/`. See `docs/README.md`.

### docs/dev/architecture.md -- System Architecture Overview

**Source material**: `codebase-guide.md`, `CLAUDE.md` architecture section

A concise (aim for 200-300 lines) overview of how the system works.
Structure:

```
# Architecture

## Pipeline overview
  (diagram: CLI -> stack-args -> YAML preprocess -> CFN operations -> output)

## YAML preprocessing engine
  Phase 1 / Phase 2 explanation
  Key abstractions: YamlAst, PreprocessingTag, TagContext, EnvValues
  Import system overview
  Handlebars integration

## CloudFormation operations
  CfnContext and the handler pattern
  run_command_handler! / await_and_render! macros
  Stack-args loading pipeline
  Request building

## Output system
  Data-driven architecture (OutputData enum)
  Renderer trait and implementations
  DynamicOutputManager

## Testing strategy
  Unit tests, integration tests, snapshot tests, property tests
  Fixture system
  example-templates/ auto-discovery
```

### docs/dev/output-architecture.md -- Output System Deep Dive

**Source material**: `notes/2025-06-17-data-driven-output-architecture.md`
(106KB -- distill to essentials)

The current doc is 106KB and reads like a work log. Extract the
architectural decisions and current design into a focused reference.
Include the section management system, spinner lifecycle, and the
command-handler/renderer separation rules.

### docs/dev/adr/ -- Architecture Decision Records

Move and condense existing ADRs:

- `docs/dev/adr/001-output-sequencing.md` -- from `ADR-2025-07-06`
- `docs/dev/adr/002-data-driven-output.md` -- from the data-driven output arch doc
- `docs/dev/adr/003-template-approval.md` -- from `template-approval-design-spec.md`

Each ADR should follow the standard format:
```
# ADR-NNN: Title
Status: Accepted
Date: YYYY-MM-DD

## Context
## Decision
## Consequences
```

### docs/dev/aws-config.md -- AWS Configuration Resolution

**Source material**: `notes/aws-config-resolution-order.md`

How AWS credentials, regions, and profiles are resolved. The interaction
between CLI flags, environment variables, stack-args environment maps,
and the AWS SDK config chain.

### docs/yaml-preprocessing.md -- YAML Preprocessing Reference (user-facing)

**Source material**: `codebase-guide.md` YAML section, existing tests

Complete reference for all 20 preprocessing tags with examples. This is
the document a user would read to understand the preprocessing language.
Structure by tag category:
- Variable lookup: `!$`
- Control flow: `!$if`, `!$let`
- Collection transforms: `!$map`, `!$concat`, `!$merge`, `!$concatMap`,
  `!$mergeMap`, `!$mapListToHash`, `!$mapValues`, `!$groupBy`, `!$fromPairs`
- String operations: `!$join`, `!$split`, `!$toYamlString`, `!$parseYaml`,
  `!$toJsonString`, `!$parseJson`
- Comparison: `!$eq`, `!$not`
- Escaping: `!$escape`
- Import types with examples

### docs/dev/js-compatibility.md -- Behavioral Differences from iidy-js

**Source material**: `codebase-guide.md` "Behavioral Differences" section,
`IIDY_JS_COMPATIBILITY_FIXES.md`

Consolidate into a single reference. For each difference: what iidy-js
does, what the Rust version does, whether the divergence is intentional.

---

## Quality Checklist

For every document produced:

- [ ] No emojis anywhere
- [ ] No filler text ("In this document we will..." / "Let's explore...")
- [ ] Opens with what the document IS, not what it will tell you
- [ ] Uses concrete examples from the actual codebase
- [ ] File paths are verified to exist
- [ ] Cross-links to related docs use relative paths
- [ ] Readable by a developer who has never seen the codebase
- [ ] Reviewed by a cold-read sub-agent that tries to answer questions

---

## Completion Status (as of 2026-02-18)

### Done

- **Part 1: Archive stale notes** -- 52 files moved to `notes/archive/`
- **Part 1: Rewrite index.md** -- rewritten with docs split into user-facing and dev sections
- **Part 3: docs/dev/architecture.md** -- created
- **Part 3: docs/dev/output-architecture.md** -- created
- **Part 3: docs/dev/adr/001-output-sequencing.md** -- created
- **Part 3: docs/dev/adr/002-data-driven-output.md** -- created
- **Part 3: docs/dev/adr/003-template-approval.md** -- created
- **Part 3: docs/dev/aws-config.md** -- created
- **Part 3: docs/yaml-preprocessing.md** -- created (user-facing)
- **Part 3: docs/dev/js-compatibility.md** -- created
- **Docs reorganization** -- split `docs/` into user-facing (top level) and dev-internal (`docs/dev/`), added `docs/README.md`
- **Cross-reference audit** -- all links in CLAUDE.md, notes/index.md, and inter-doc refs verified and fixed
- **Part 2: Audit docs/SECURITY.md** -- verified
- **Part 2: Audit docs/dev/COVERAGE.md** -- `make coverage-*` targets confirmed present in Makefile
- **codebase-guide.md** -- moved to `docs/dev/codebase-guide.md`, symlinked from `notes/`
- **aws_api_analysis.org** -- already archived to `notes/archive/`
