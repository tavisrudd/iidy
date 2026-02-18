# Handoff: Code Quality Review

**Date**: 2026-02-17
**For**: Next Claude instance orchestrating a multi-agent code review

## Goal

Conduct a detailed code quality review of the three main subsystems and
their tests. Produce a single consolidated findings document at
`notes/2026-02-17-code-review-findings.md`.

## Project Context

This is a Rust rewrite of iidy, a CloudFormation deployment tool. 608 tests
pass, zero warnings. The codebase is mature but hasn't been touched in ~7
months. We're about to add a significant new feature (custom resource
templates -- see `notes/2026-02-17-custom-resource-templates-rfc.md`), so
this review should surface issues worth fixing before that work begins.

Key references:
- `notes/codebase-guide.md` -- full navigation guide with file paths
- `notes/2026-02-17-project-review-and-next-steps.md` -- known bugs
- `CLAUDE.md` -- project standards and coding conventions

## Agent Plan

Launch 5 sub-agents in parallel. Each should return structured findings
with file paths, line numbers, severity (high/medium/low), and a one-line
description. After all return, synthesize into the findings doc.

### Agent 1: src/yaml/ code quality (Sonnet)

Prompt focus:
- Read `src/yaml/resolution/resolver.rs` (the largest file, ~2200 lines).
  Look for: functions that are too long, duplicated patterns that should
  be extracted, match arms with copy-pasted logic, error handling that
  could lose context.
- Read `src/yaml/parsing/parser.rs`. Same concerns. Check that all
  `PreprocessingTag` variants have consistent parsing patterns.
- Read `src/yaml/engine.rs`. Check the Phase 1/Phase 2 boundary. Is the
  re-serialization round-trip in `process_imported_document` (Value ->
  String -> YamlAst) necessary or avoidable?
- Read `src/yaml/errors/wrapper.rs`. The 18 `TODO: PANIC POTENTIAL`
  markers -- which are genuine risks vs already-guarded false alarms?
- Check `src/yaml/handlebars/engine.rs` -- the registry-per-call issue.
- Check `src/yaml/imports/loaders/` -- any `unwrap()` on user-controlled
  input?

### Agent 2: src/output/ code quality (Sonnet)

Prompt focus:
- Read `src/output/renderers/interactive.rs` (~1700 lines). Look for:
  overly long functions, duplicated rendering patterns, TODO items.
- Read `src/output/renderers/json.rs`. Completeness -- does every
  `OutputData` variant have a handler?
- Read `src/output/manager.rs`. The event buffer replay on mode switch --
  any issues with the 1000-event cap? What happens when it overflows?
- Read `src/output/keyboard.rs`. The emoji violations (project bans
  emojis). The `ToggleTimestamps` stub.
- Read `src/output/data.rs`. Are the 25 `OutputData` variants well-
  structured? Any that should be consolidated?
- Read `src/output/aws_conversion.rs`. The hardcoded `None`/`false`
  fields -- are they documented?

### Agent 3: src/cfn/ code quality (Sonnet)

Prompt focus:
- Read `src/cfn/mod.rs` (645 lines). The `run_command_handler!` and
  `await_and_render!` macros -- are they well-designed or hiding
  complexity? Are they used consistently?
- Read `src/cfn/stack_args.rs` (699 lines). The stack-args loading
  pipeline -- any edge cases in environment-map resolution?
- Read `src/cfn/changeset_operations.rs` (561 lines). The largest
  command handler -- any duplication with other handlers?
- Read `src/cfn/request_builder.rs` (655 lines). The request building
  pattern -- consistent? Any missing validation?
- Scan all command handlers (create_stack, update_stack, delete_stack,
  etc.) for: inconsistent error handling, missing exit codes, duplicated
  boilerplate that the macros should have eliminated.

### Agent 4: Test quality review (Sonnet)

Prompt focus:
- Read `tests/` directory. Are there tests that are too coupled to
  implementation details? Tests that test nothing meaningful?
- Check snapshot tests in `tests/snapshots/` -- any that are overly
  brittle (would break on cosmetic changes)?
- Read `tests/yaml_tests.rs` and `tests/yaml_preprocessing_integration.rs`
  -- do they cover the edge cases that matter? Any obvious gaps?
- Read `tests/output_renderer_snapshots.rs` and
  `tests/fixture_validation_tests.rs` -- how robust is the output testing?
- Check for test helper duplication across test files.
- Look at `tests/property_tests.rs` -- are the property tests meaningful
  or superficial?

### Agent 5: Concurrency review (Opus 4.6)

This one needs careful reasoning. Prompt focus:
- The project uses `tokio` async runtime. Read `src/main.rs` to
  understand the runtime setup.
- Read `src/cfn/mod.rs` for `CfnContext` -- is it `Send`/`Sync`? Is it
  shared across tasks?
- Read `src/cfn/stack_operations.rs` -- `StackInfoService` and
  `StackEventsService`. These make concurrent AWS calls. Any races?
- Read `src/output/manager.rs` -- `DynamicOutputManager`. Multiple
  tasks can emit `OutputData`. Is the rendering path thread-safe?
- Read `src/output/keyboard.rs` -- `KeyboardListener` spawns a tokio
  task. How does it communicate with the main task? Any channel issues?
- Read `src/yaml/engine.rs` -- `YamlPreprocessor` uses `async` for
  import loading. Are imports loaded concurrently? Any shared mutable
  state?
- Read `src/aws/` -- credential management, NTP timing. Any global
  mutable state?
- Look for: `Arc<Mutex<>>` patterns, `tokio::spawn` without join
  handles being tracked, `static mut`, `unsafe`, `once_cell` usage,
  channels that could deadlock.

## Orchestration Notes

- All agents except #5 should use `model=sonnet` for speed/cost.
- Agent #5 (concurrency) should use `model=opus` for deeper reasoning.
- Each agent should read the actual source files, not guess from names.
- Tell each agent to format findings as a markdown list with:
  `- **[severity]** `file:line` -- description`
- After all 5 return, write findings to
  `notes/2026-02-17-code-review-findings.md`, organized by subsystem
  with a summary of the most important issues at the top.
- Do NOT fix any issues. This is a review only. Fixes come later.
