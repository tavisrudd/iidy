# Handoff: README and Docs Rebalance

**Date**: 2026-02-19
**For**: Next Claude instance to continue doc review and fixes
**Status**: Complete pending user review

## What Was Done

Rewrote README.md, docs/getting-started.md, the intro of
docs/yaml-preprocessing.md, and added a section to docs/command-reference.md.
Goal: rebalance emphasis away from preprocessing and toward iidy's operational
value layers:

1. **Polished CLI experience** -- fast, readable feedback on every operation
2. **stack-args.yaml** -- simple parameterization without wrapper scripts
3. **Changeset workflow** -- review before committing to production updates
4. **Template approval** -- multi-team sign-off backed by S3 versioning
5. **Preprocessing** -- positioned as optional, for growing complexity

### Files modified

- `README.md` -- full rewrite
- `docs/getting-started.md` -- full rewrite
- `docs/yaml-preprocessing.md` -- intro rewritten (lines 1-47)
- `docs/command-reference.md` -- added Idempotency Tokens section after Exit Codes

### README.md changes
- Leads with CLI feedback, stack config, changesets, approval
- Added asciicast embed from iidy-js
- Quick start uses `Template: ./cfn-template.yaml` (no `render:`)
- Added full output example block showing create-stack output
- Describes interactive + CI dual-use (--output-mode, --yes, TTY detection)
- Preprocessing section describes language design: "purely functional,
  data transformations on valid YAML, not string-based templating"
- Notes `iidy render` used outside CFN for k8s manifests etc.

### docs/getting-started.md changes
- First deployment uses plain CFN template (no render: prefix)
- Added CLI output examples for: create-stack, describe-stack, update-stack,
  list-stacks, delete-stack
- SSM examples use non-secret config only (cluster-size, domain-name)
- No secrets in any parameter example anywhere
- render: prefix introduced late, positioned as optional
- Output modes section explains interactive + CI dual-use

### docs/yaml-preprocessing.md changes
- Intro describes language design philosophy (data transformations, not strings)
- Document structure example simplified (removed !$join from $defs)
- Notes `iidy render` for non-CFN use (k8s, CI configs)

### docs/command-reference.md changes
- Added Idempotency Tokens section explaining auto-generated client request
  tokens, deterministic derivation for multi-step ops, and user-provided
  tokens for semantic tracking (release tags, CI build IDs)
- Rewrote Template Approval section with full workflow explanation (5-step
  process), security model (IAM on the deploy role restricts CFN mutations
  to approved S3 templates -- the actual gate), S3 bucket permission breakdown
  (developers submit .pending, reviewers approve, CFN service role reads
  approved only), cross-account ACL handling
- Fixed review command examples to use S3 URIs (not HTTPS URLs)

## Fact-Check Process and Corrections

Three Opus sub-agents performed comprehensive fact-checks against the renderer
source code. Many issues found and fixed:

### Output format corrections
- **Timestamp format**: Was `YYYY-MM-DD HH:MM:SS`, corrected to `%a %b %d %Y %H:%M:%S`
  (e.g., "Wed Jan 14 2026 11:14:15"). Source: `render_timestamp` at interactive.rs:211
- **Event column order**: Was `timestamp status logical_id type`, corrected to
  `timestamp status resource_type logical_id`. Source: `render_single_stack_event` at
  interactive.rs:1088-1095
- **Event table headers/separators**: Removed fabricated header row and dashed
  separator lines. The renderer produces no table chrome.
- **Event sub-lines**: Removed fabricated "Duration:" and "Reason:" sub-lines from
  successful events. Renderer only shows reason for FAILED events.
- **Command result format**: Was `Command Result: SUCCESS (Ns)`, corrected to
  `SUCCESS: (Ns)`. Source: `render_command_result` at interactive.rs:1572-1581
- **Live events header**: Was `Live Stack Events:`, corrected to
  `Live Stack Events (2s poll):`. Source: `configure_section_titles` at interactive.rs:734
- **Confirmation prompt**: Was `CONFIRMATION REQUIRED: msg`, corrected to
  `? msg (y/N)`. The CONFIRMATION REQUIRED format is plain/non-interactive only.
  Source: `render_confirmation_prompt` at interactive.rs:1990-2020
- **Delete confirmation text**: Was "Delete stack X?", corrected to
  "Are you sure you want to DELETE the stack X?"
  Source: `delete_stack.rs`:180

### Fabricated features removed
- **"Collapsible sections and keyboard navigation"**: This feature does not
  exist. The interactive renderer is a linear streaming text renderer with
  colors and spinners, not a TUI. Removed from both README and getting-started.
- **"Press j/k/q/h"**: No keyboard input handling exists beyond y/n confirmation
  prompts. Removed.
- **"Parameter Changes:" section**: No such section exists in the renderer.
  Removed from update-stack example.

### Structural corrections
- **update-stack confirmation**: Regular `update-stack` (without `--changeset`)
  has NO confirmation prompt. Only `--changeset` path has one. Corrected the
  getting-started walkthrough.
- **--yes flag placement**: `--yes` is a subcommand-level flag on update-stack
  and delete-stack, not a global flag. Fixed example from
  `iidy --yes update-stack` to `iidy update-stack ... --yes`.
- **describe-stack sections**: describe-stack has no Command Metadata section
  (only stack_definition, stack_events, stack_contents). Fixed example.
- **Stack Resources**: Added missing Stack Resources section to create-stack
  output examples (renderer always outputs it before Stack Outputs).
- **Current Stack Status**: Added to describe-stack example (renderer always
  outputs it).

### Security corrections
- Removed database password from SSM import example (secrets should never be
  passed as CFN parameters -- they're visible in console, API, CloudTrail).
  Replaced with non-secret config values (cluster-size, domain-name).
- User directive: keep secrets handling out of intro material entirely.

### Other corrections
- **Timestamps**: Changed from future dates (June 2026) to past dates
  (January 2026). Verified day-of-week for all dates.
- **Durations**: Made realistic (S3 bucket ~8s, not 45s).
- **Import types list**: Added http/https (was missing).
- **Output examples**: Expanded to show full field set rather than abbreviated
  subset (Command Metadata shows all 9 fields, Stack Details shows all fields
  through Console URL).

### Confirmed accurate
- "purely functional, side-effect-free once imports are loaded" -- confirmed.
  filehash I/O happens during import phase, not resolution. No handlebars
  helper or tag does I/O during resolution.
- asciicast URL matches iidy-js README
- All stack-args.yaml field names match StackArgs struct
- render command has no CFN dependency (standalone YAML preprocessor)
- CFN tag pass-through behavior documented correctly

## Remaining Minor Issues

1. **list-stacks timestamps right-aligned** in real output (`:>width` in code)
   but left-aligned in docs. Minor visual difference.

2. **Test fixture discrepancy**: tests/fixtures/create-stack-happy-path.yaml
   does NOT match renderer output. It has table headers, separator lines,
   Duration/Reason/Physical ID sub-lines, and wrong column order. The
   pixel-perfect tests only check string containment, not stdout capture.
   This is a separate issue from documentation.

## Lessons for Future Doc Work

- **Always fact-check output examples against renderer source code.** The
  initial doc generation produced plausible but incorrect output format.
  Sub-agents are useful for broad fact-checking but can also hallucinate
  issues (e.g., the filehash I/O claim was wrong).
- **Don't describe aspirational features.** "Collapsible sections" and
  "keyboard navigation" appeared in architecture docs as design goals but
  were never implemented. User-facing docs must describe current behavior.
- **Security review every example.** CFN parameters are not secret-safe.
  Never show secrets in parameter examples, and keep secrets out of intro
  material entirely.
- **CFN API semantics matter.** Client request tokens are optional (supported,
  not required). Get the distinction right.
- **Verify day-of-week when using named timestamps.** The `%a %b %d %Y`
  format includes weekday names that must be correct for the date shown.
- **--yes and similar flags are per-subcommand, not global.** Check cli.rs
  to verify flag scope before showing examples.
