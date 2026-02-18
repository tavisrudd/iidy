# Code Review Findings

**Date**: 2026-02-17
**Purpose**: Pre-feature code quality review before custom resource template implementation

## Executive Summary

Five-agent parallel review covering: YAML engine, output system, CFN operations,
test quality, and concurrency. The codebase is structurally sound (608 tests, zero
warnings) but has accumulated technical debt worth addressing before adding the
custom resource template feature.

**Totals**: ~30 high, ~45 medium, ~30 low severity findings across all subsystems.

**Critical themes:**

1. **Test suite has significant dead weight** -- ~20 test functions contain zero
   meaningful assertions (tautologies, placeholder snapshots, debug-only prints).
   Output renderer testing is almost entirely illusory: tests verify data struct
   construction, not rendered output. Multiple "pixel perfect" and "fixture
   validation" tests assert against fixture content, never renderer output.

2. **Emoji violations throughout output code** -- At least 12 instances across
   interactive.rs, keyboard.rs. Direct violation of project standards.

3. **Command handler inconsistency** -- 2 handlers bypass the macro system
   entirely (get_stack_template, get_import); several others
   duplicate error-handling boilerplate the macros were designed to eliminate.
   Watch-error path (convert_aws_error + render + Ok(1)) copy-pasted in 4+
   handlers.

4. **Silent data inaccuracy** -- `disable_rollback: false` hardcoded in AWS
   conversion (actual stack config ignored); changeset status hardcoded to
   `CREATE_COMPLETE` regardless of actual status.

5. **YAML engine re-serialization round-trip** -- `process_imported_document`
   serializes Value back to string then re-parses, destroying source locations
   and dropping parent's AWS import loader config. Nested documents with S3/SSM
   imports will fail silently.

6. **Blocking I/O on async runtime** -- `fs::read_to_string` in stack_args.rs
   and resolver.rs (error paths), `exists()` in aws/mod.rs, `crossterm::event::poll`
   in keyboard.rs all block tokio worker threads.

7. **Unsafe env var mutation** -- `set_var` called inside async context with tokio
   threads alive. Safety comment is incorrect. Potential UB per Rust 2024 rules.

8. **Handlebars registry per-call rebuild** -- Known perf issue confirmed. 100
   interpolations = 100 registry allocations with ~20 boxed helpers each.

9. **Duplicated code across subsystems** -- Pending changeset fetching duplicated
   between changeset_operations.rs and stack_operations.rs. Timestamp conversion
   duplicated 3x in aws_conversion.rs. Truthiness logic duplicated 3x in
   resolver.rs. Test helpers duplicated across 3 test files.

10. **Parser silently drops tag errors** -- `build_tagged_node` converts
    `parse_preprocessing_tag` errors to `UnknownYamlTag`. Typos in `!$` tag names
    are silently ignored. Several tag parsers silently ignore unknown fields.

---

## src/output/ -- Output System

### Renderers

- **[high]** `interactive.rs:1257-1285` -- `render_final_command_summary` embeds emoji in both color-enabled and color-disabled paths ("Success :thumbsup:", "Failure (table flip)"). Project bans emojis everywhere.

- **[high]** `interactive.rs:1035` -- `render_stack_definition` embeds lock emoji on TerminationProtection line. Appears in production output.

- **[high]** `interactive.rs:1313,1315,1317` -- `render_stack_list` uses lock emoji, infinity, recycle symbols as lifecycle icons.

- **[high]** `interactive.rs:1889,1930` -- `render_approval_request_result` and `render_approval_status` embed thumbsup emoji.

- **[high]** `interactive.rs:1910,1917,1922` -- `render_template_validation` uses non-ASCII symbols (check, cross, warning) as status indicators.

- **[medium]** `interactive.rs:447-538` -- `setup_operation` is 91 lines combining two responsibilities: setting `expected_sections` and calling `configure_section_titles`. DeleteStack duplicates section lists between `args.yes` branches.

- **[medium]** `interactive.rs:576-637` -- `configure_section_titles` has inconsistent trailing colon on title strings between operations. UpdateStack and CreateChangeset fall through to default arm, losing the operation-specific colon.

- **[medium]** `interactive.rs:1104-1214` -- `render_stack_contents` is 111 lines handling five sub-sections inline. Pending changeset loop re-implements logic from `render_changeset_result`.

- **[medium]** `interactive.rs:1855-1885` and `interactive.rs:1679-1713` -- `render_stack_absent_error_with_context` and `render_stack_absent_info` are structurally identical. Only difference is prefix label and color.

- **[medium]** `interactive.rs:907-908` -- `render_single_stack_event` applies coarse `reason.replace("Initiated", "")` with a `TODO: review` comment. Will incorrectly strip "Initiated" from legitimate resource reason strings.

- **[low]** `interactive.rs:1073-1101` -- `render_stack_events` inspects `data.title.contains("Live Stack Events")` to special-case display. Couples renderer to specific title string.

- **[low]** `interactive.rs:1516-1518` -- `render_token_info` silently discards TokenInfo with `let _ = data` and a TODO about verbosity flag.

- **[low]** `interactive.rs:215-217` -- `render_event_timestamp` is an alias for `render_timestamp` with no transformation.

### JSON Renderer

- **[medium]** `json.rs:109-149` -- `render_stack_template` writes raw text to stderr/stdout, bypassing JSONL contract. JSON consumers receive unstructured text interleaved with JSON objects.

- **[medium]** `json.rs:241-261` -- `render_confirmation_prompt` uses `"confirmation_required"` as type field instead of `"confirmation_prompt"` matching the OutputData discriminant name.

- **[low]** `json.rs:371` -- Test contains `assert!(true)`.

### Output Manager

- **[medium]** `manager.rs:67-69` -- Buffer overflow (1000 events) silently drops oldest events with no warning or counter. Mode-switch replay after overflow will be silently incomplete.

- **[medium]** `manager.rs:92-95` -- Mode-switch replay clones ConfirmationRequest events but `Clone` impl sets `response_tx` to `None`. Replayed confirmation prompts have dead channels; interactive renderer will re-prompt user on mode switch.

### Keyboard

- **[high]** `keyboard.rs:145` -- `show_help` prints clipboard emoji.

- **[high]** `keyboard.rs:233` -- Prints lightbulb emoji in unimplemented message.

- **[medium]** `keyboard.rs:23,231-233` -- `ToggleTimestamps` is fully dead-end wired: enum variant, keybinding, help text all exist, but handler does nothing. No mechanism to propagate toggle to renderers.

- **[low]** `keyboard.rs:97` -- Background task hardcodes 50ms poll interval, ignoring `config.poll_interval` field.

- **[low]** `keyboard.rs:264` -- Test asserts `is_tty == true || is_tty == false` (tautology).

### Data Types

- **[medium]** `data.rs:492-518` -- `OutputData::StackDefinition(StackDefinition, bool)` uses anonymous bool for `show_times`. Not self-documenting at call sites.

- **[medium]** `data.rs:473-488` -- `ConfirmationRequest` custom Clone impl silently nulls `response_tx`. Breaks semantic contract of Clone.

- **[low]** `data.rs:503-504` -- `NewStackEvents` vs `StackEvents` distinction is implicit. Naming like `HistoricalStackEvents` / `LiveStackEventBatch` would be clearer.

- **[low]** `data.rs:276` -- `StackListColumn::from_str` hand-rolls what `clap`'s `ValueEnum` derive generates.

- **[low]** `data.rs:408-417` -- `CostEstimate` wraps `CostEstimateInfo` with no added value. Unnecessary nesting.

### AWS Conversion

- **[high]** `aws_conversion.rs:286` -- `disable_rollback: false` hardcoded with TODO. AWS SDK exposes this field. Users always see false regardless of actual stack config.

- **[medium]** `aws_conversion.rs:273` -- `stackset_name: None` hardcoded. Renderer works around it via tag lookup, but the two detection mechanisms are inconsistent and untested.

- **[medium]** `aws_conversion.rs:292` -- `stack_policy: None` hardcoded. JSON consumers always receive null with no indication of intentional omission.

- **[medium]** `aws_conversion.rs:176-183,246-252` -- Timestamp conversion pattern (AWS DateTime -> chrono) duplicated 3 times. Should be a shared helper.

- **[medium]** `aws_conversion.rs:204-212` -- Environment type detection duplicated between conversion layer and `render_stack_list`.

---

## src/cfn/ -- CloudFormation Operations

### Core Infrastructure

- **[high]** `mod.rs:288-297` -- `create_context` is dead code, superseded by `create_context_for_operation`. Lacks region validation guard present in newer helpers.

- **[high]** `mod.rs:479` -- `apply_stack_name_override_and_validate` is followed by redundant `stack_name.ok_or_else(...)` checks in 5+ callers. Dead defensive code obscures the invariant.

- **[medium]** `mod.rs:354-357` -- `get_start_time` is an unnecessary async wrapper around a field access.

- **[medium]** `mod.rs:397-406` -- `get_used_tokens` silently returns empty Vec on mutex poisoning. Token audit trail lost with only a log::warn.

- **[low]** `mod.rs:166` -- Commented-out `pub mod console;` line. Should be removed per project convention.

### Stack Args

- **[high]** `stack_args.rs:100-101` -- `fs::read_to_string` blocks tokio thread inside async function. Should use `tokio::fs::read_to_string`.

- **[high]** `stack_args.rs:168-205` -- Double preprocessing pass re-serializes YAML then re-preprocesses. Expanded values that look like preprocessing tags can be silently corrupted. Three total preprocessing passes make pipeline hard to reason about.

- **[high]** `stack_args.rs:295-335` -- `apply_global_configuration` makes live SSM API call unconditionally. Tests calling `load_stack_args` hit SSM on every run, violating offline testing requirement.

- **[medium]** `stack_args.rs:59-75` -- `resolve_env_map` handles only Profile/AssumeRoleARN/Region. StackName/Template env maps produce confusing deserialization errors.

- **[medium]** `stack_args.rs:437` -- `println!` bypasses output manager, breaks structured output mode.

- **[medium]** `stack_args.rs:475` -- Hardcoded `/bin/bash` path. Does not exist on NixOS.

- **[low]** `stack_args.rs:12` -- Blanket `#[allow(dead_code)]` on StackArgs struct.

- **[low]** `stack_args.rs:233` -- Local import inside function violates project coding standards.

### Changeset Operations

- **[high]** `changeset_operations.rs:110` -- `stack_args.stack_name.as_ref().unwrap()` can panic. Same pattern at 4+ other locations across handlers.

- **[high]** `changeset_operations.rs:369-434` -- `fetch_pending_changesets` duplicated nearly verbatim in `stack_operations.rs:94-168` (`collect_pending_changesets`).

- **[high]** `changeset_operations.rs:336` -- `build_changeset_result` hardcodes `status: "CREATE_COMPLETE"` regardless of actual status.

- **[medium]** `changeset_operations.rs:501-502` -- Console URL generator inconsistent with `generate_changeset_console_url`.

- **[medium]** `changeset_operations.rs:195-306` -- `build_create_changeset_with_type` near-duplicates `CfnRequestBuilder::build_create_changeset` in request_builder.rs.

- **[medium]** `changeset_operations.rs:526-559` -- `confirm_changeset_execution` uses boolean flag to select between two code paths. Should be two functions.

- **[low]** `changeset_operations.rs:67-70` -- `check_existing_changesets` swallows all errors and returns `"unknown-changeset"`.

### Request Builder

- **[high]** `request_builder.rs:306-389` -- `build_create_changeset` is sync (not async), so it cannot call `load_cfn_template`. Templates >51KB cannot be auto-uploaded to S3. Template preprocessing skipped. S3 URL templates inlined verbatim.

- **[medium]** `request_builder.rs:127-132,213-215,280-284` -- `service_role_arn` vs `role_arn` fallback logic duplicated 3 times.

- **[medium]** `request_builder.rs:84-83,135-137` -- Missing comments explaining why certain fields are absent from specific builder methods (stack_policy on changeset, timeout on update). Correct omissions that look like oversights.

### Command Handler Consistency

- **[high]** `get_stack_template.rs:96-143` -- Does not use `run_command_handler!`. Manually reconstructs output manager setup inline.

- **[high]** `get_import.rs:32-126` -- Bypasses output manager entirely. Writes directly to stdout/stderr with print!/println!/eprintln!. Incompatible with structured output.

- **[high]** `create_stack.rs:50-69` -- Watch-loop error path (5 lines: convert_aws_error + render + Ok(1)) duplicated in 4+ handlers. `await_and_render!` macro not applied to watch-error paths.

- **[medium]** `create_stack.rs:73-84` -- String literal `"DELETE_COMPLETE"` used instead of `DELETE_SUCCESS_STATES` constant. Same magic string in 5+ locations.

- **[medium]** `exec_changeset.rs:40-66` -- Manual StackEventWithTiming construction duplicates `convert_stack_events_to_display_with_max` from aws_conversion.rs.

- **[medium]** `update_stack.rs:86` -- Accepts `_stack_name: &str` parameter but never uses it. Re-extracts from stack_args via unwrap().

- **[medium]** `describe_stack_drift.rs:75-87` -- Drift polling loop has no timeout and no max iteration count. Stuck detection spins forever.

- **[medium]** `create_or_update.rs:89` -- `UpdateNoChanges` path returns Ok(0) with no final command summary, unlike all other return paths.

- **[low]** `stack_operations.rs:313-326` -- Stack-exists detection uses substring matching on error strings. Inconsistent with changeset_operations.rs which uses error code checking.

- **[low]** `watch_stack.rs:87` -- Full events vector cloned just to populate seen HashSet. Could pass only event IDs.

---

## tests/ -- Test Quality

### Dead Tests (zero meaningful assertions)

- **[high]** `yaml_tests.rs:20,64,112,252` -- Four tests parse YAML and assert only `matches!(ast, YamlAst::Mapping(_,_))` or `content.contains(":")`. Zero semantic verification. Multi-screen TODO comments indicate these are stubs from early development.

- **[high]** `yaml_tests.rs:190` -- `test_parsing_consistency_across_fixtures` parses each fixture twice and checks `is_ok() == is_ok()`. Empty if-body. Tests nothing beyond "parser doesn't crash".

- **[high]** `property_tests.rs:325` -- Contains `assert!(!template.is_empty() || template.is_empty())` -- a tautology that literally cannot fail.

- **[high]** `property_tests.rs:76` -- `prop_handlebars_engine_idempotent` silently exits without asserting when template contains `{{`. Half the strategy's output untested.

- **[high]** `property_tests.rs:57` -- Seven proptest properties are stubs: parse a value, assert parsing succeeds, never resolve or compare output. All carry "NOTE: Once AST resolution is implemented..." comments.

- **[high]** `nested_import_debug.rs` (entire file) -- All four tests contain zero assertions. Return `Ok(())` after debug printing. Committed exploratory code.

- **[medium]** `tree_sitter_debug.rs`, `tree_sitter_tag_debug.rs` (entire files) -- All tests contain only `println!` calls with no assertions.

- **[medium]** `property_tests.rs:248` -- `test_property_test_framework_works` contains only `assert!(true)`.

### Output Renderer Test Illusion

- **[high]** `output_renderer_snapshots.rs:458,466,474` -- Three snapshot tests snapshot hardcoded placeholder strings, not actual renderer output.

- **[high]** `output_renderer_snapshots.rs:49-416` -- Every test in this file renders data to the renderer but only asserts against the data structure it built, never against renderer output. Renderer could produce garbage and all tests pass.

- **[high]** `fixture_validation_tests.rs:78` -- Test titled "against fixture expected output" never compares renderer output to fixture. Only checks fixture has content.

- **[high]** `pixel_perfect_output_tests.rs:46-200` -- All four "pixel perfect" tests assert against strings in the fixture file, not renderer output. Name is misleading.

### Silenced Failures

- **[medium]** `yaml_preprocessing_integration.rs:311` -- Assertion commented out with "handlebars isn't working properly, let's skip this assertion for now". Debug println left in.

- **[medium]** `yaml_preprocessing_integration.rs:258` -- Two assertions for nested import access commented out as "not fully implemented yet".

- **[medium]** `input_uri_traversal_tests.rs:1412` -- Neither Ok nor Err outcome causes test failure. Comment admits "Both behaviors are potentially valid".

### Test Helper Duplication

- **[medium]** `create_test_plain_options()` / `create_plain_options()` and `create_test_interactive_options()` / `create_interactive_options()` duplicated across 3 test files.

### Miscellaneous

- **[medium]** `enhanced_error_reporting_tests.rs:54` -- Multiple tests use emoji in println output, violating project standards.

- **[medium]** `enhanced_error_reporting_tests.rs:272` -- Test validates a local helper function, not production error formatting.

- **[medium]** `error_reporting_tests.rs:32` -- Assertion so broad almost any error message containing "variable" would pass.

- **[low]** `equivalence_tests.rs:84` -- `eprint!` dumps every test case to stderr during normal runs.

- **[low]** Snapshot duplication: 8 `example_templates_snapshots__` snapshots appear superseded by `auto_discovered_` equivalents.

---

## src/yaml/ -- YAML Engine

### Resolver (resolver.rs ~2200 lines)

- **[high]** `resolver.rs:535-543` -- Dead-branch logic bug in `process_string_with_handlebars`: the `else` branch of `if let Some(input_uri)` re-checks `context.input_uri.as_deref()` which is always `None` in that branch. Result is always `"unknown location"`. Same pattern repeated at lines 579-587.

- **[high]** `resolver.rs:546` -- `std::fs::read_to_string` called inside error handler for every variable-not-found error. Synchronous I/O inside async runtime context. Re-reads entire source file on every error path. Same pattern at lines 939, 981.

- **[high]** `resolver.rs:520-576` -- Error message parsing by string-searching handlebars error text (`error_msg.contains("Variable")`, parsing variable name by scanning for `Variable "`). Brittle coupling to dependency's error message format. Falls back to `"unknown"` silently if format changes.

- **[medium]** `resolver.rs:402-421` -- `resolve_dot_notation_path` checks `is_none()` then immediately calls `unwrap()`. Should be `if let`.

- **[medium]** `resolver.rs:1032-1040` -- `resolve_if` reimplements truthiness logic inline instead of calling `self.is_truthy()`. Same 6-arm match duplicated verbatim in `resolve_not` (lines 1207-1215).

- **[medium]** `resolver.rs:1384-1426` -- `resolve_concat_map` clones all fields from `ConcatMapTag` to construct a temporary `MapTag`. Same wasteful clone in `resolve_merge_map` (lines 1429-1483).

- **[medium]** `resolver.rs:719-745` -- Merge-key detection in `resolve_mapping` duplicates logic already present in parser (`parser.rs:602-605`). Different error formats.

- **[medium]** `resolver.rs:1546-1557` -- `resolve_map_list_to_hash` constructs inline type name match instead of using existing `ValueTypeStr` trait.

- **[low]** `resolver.rs:1600-1626` -- `resolve_group_by` collects into `HashMap` (non-deterministic order). Known bug.

- **[low]** `resolver.rs:627-631` -- `!$escape` on preprocessing tags returns hardcoded `"!$escaped_tag"` string. Known bug.

### Parser (parser.rs)

- **[high]** `parser.rs:969-975` -- `build_tagged_node` silently converts any `parse_preprocessing_tag` error to `UnknownYamlTag`. Typos in `!$` tag names (e.g., `!$mapp`) are silently dropped. Inconsistent with `build_block_node` and `build_flow_node` which propagate errors correctly.

- **[medium]** `parser.rs:1530-1534` -- `parse_map_list_to_hash_tag` calls `extract_field_from_mapping(...).unwrap()` after `validate_tag_fields`. But validation only checks `Mapping` content; non-Mapping input passes validation silently, then `unwrap()` panics. Reachable via `!$mapListToHash someString`.

- **[medium]** `parser.rs:1388` -- `parse_map_values_tag` ignores unknown fields silently. Misspelled keys like `itesm:` dropped with no error. Inconsistent with `parse_map_tag` which rejects unknown fields.

- **[medium]** `parser.rs:1601-1635` -- `parse_if_tag` also ignores unknown fields. User writing `!$if` with misspelled `tset:` gets no error; condition resolves to null.

- **[medium]** `parser.rs:810-862,864-940` -- `build_block_node` and `build_flow_node` contain ~30 lines of copy-pasted tag classification logic. Should be a shared helper.

- **[medium]** `parser.rs:1816-1863` -- `extract_file_path` has hardcoded `"example-templates/"` substring check. Test artifact embedded in production code. Path stripping logic duplicated across multiple formatting methods.

- **[low]** `parser.rs:439` -- `analyze_syntax_error` uses fragile heuristic for unbalanced quote detection.

### Engine (engine.rs)

- **[high]** `engine.rs:368-397` -- `process_imported_document` round-trips Value -> String -> YamlAst. Destroys source location information, loses YAML comments, loses ordering guarantees. Creates new `ProductionImportLoader` instead of inheriting parent's loader.

- **[high]** `engine.rs:394-396` -- New `ProductionImportLoader::new()` created unconditionally for nested documents. AWS imports (S3, SSM, CFN) in nested docs will fail even when parent had AWS credentials via `with_aws_config()`.

- **[medium]** `engine.rs:494-520` -- `convert_yaml_12_to_11_compatibility_with_context` allocates new `Vec<String>` for path on every recursive call. O(depth) allocations per node.

- **[medium]** `engine.rs:214-217` -- `process_defs` rebuilds TagContext from scratch for each `$defs` entry by cloning all previously inserted variables. O(n^2) for n definitions.

- **[low]** `engine.rs:529-531` -- Commented-out code referring to deleted `split_args_resolver` field.

### Error Wrapper (errors/wrapper.rs) -- TODO: PANIC POTENTIAL Audit

Of 18 `TODO: PANIC POTENTIAL` markers:

**Genuine risks (2):**
- **[medium]** `wrapper.rs:322` -- `lines[line_num - 2]` where guard evaluates `line_num - 2` before catching `line_num == 0`. Usize underflow (wrap to MAX in release, panic in debug). Fix: guard should be `line_num >= 2`.
- **[high]** `wrapper.rs:967` -- `line.chars().nth(tag_end)` where `tag_end` is a byte offset. `chars().nth()` takes a character index. For non-ASCII input, returns wrong character or None. Logic error producing incorrect column highlighting.

**False alarms (confirmed safe, 16):**
- `wrapper.rs:62` -- Safe: guard `line_num > 0 && line_num <= lines.len()` prevents underflow.
- `wrapper.rs:336` -- Safe: same guard pattern.
- `wrapper.rs:360` -- Safe: `col` from `find()` is valid byte offset.
- `wrapper.rs:547` -- Safe: `bracket_pos` from `find('[')` is ASCII single-byte.
- Lines 104, 908, 997 -- Safe: `display_with_context` has correct guards.
- Lines 552, 565, 609, 624, 639, 654 -- Safe: arithmetic on column numbers. Overflow unreachable on 64-bit.

**Additional issues:**
- **[low]** `wrapper.rs:38-52` -- File-path splitting logic duplicated in 3 functions with identical TODO comments. Should be a shared helper.

### Handlebars Engine

- **[high]** `handlebars/engine.rs:76` -- `create_handlebars_registry()` called on every `interpolate_handlebars_string` invocation. Allocates registry, registers ~20 helpers with Box::new, discards after one render. Known perf issue. 100 interpolations = 100 registries.

### Import Loaders

- **[medium]** `loaders/file.rs:139` -- `hex::decode(&hash).unwrap()` on sha256 output. Safe in practice but not self-evident.

- **[low]** `loaders/file.rs:75` -- `strip_prefix('?').unwrap()` after `starts_with('?')` guard. Could use `trim_start_matches`.

- **[low]** `loaders/ssm.rs:180,199` -- `strip_prefix("ssm:").unwrap()` after starts_with guard. Should use strip_prefix return value from guard.

- **[low]** `loaders/s3.rs:99`, `loaders/cfn.rs:179,183` -- Same pattern: strip_prefix + unwrap after starts_with guard.

---

## Concurrency Review

### Unsafe Environment Variable Mutation

- **[high]** `aws/mod.rs:119-121` -- `unsafe { std::env::set_var("AWS_SDK_LOAD_CONFIG", "1") }` in async context. Safety comment claims "before any threads are spawned" but tokio worker threads are alive when this runs (inside `rt.block_on()`). Potential undefined behavior per Rust's safety rules for `set_var`. Risk is limited in practice (called during init before any `tokio::spawn` in the same command) but the safety invariant as documented is incorrect.

### Keyboard Listener

- **[medium]** `keyboard.rs:97` -- Blocking `crossterm::event::poll()` called inside `tokio::spawn`, blocking the tokio worker thread for up to 50ms per iteration. Should use `spawn_blocking` or `crossterm::event::EventStream`.

- **[medium]** `keyboard.rs:95-109` -- Spawned keyboard task has no cancellation mechanism. `stop()` only disables raw mode; does not abort the task. Task continues polling until `KeyboardListener` struct is dropped.

### Output Manager

- **[medium]** `manager.rs:70` -- `ConfirmationRequest::clone()` loses `oneshot::Sender`, making replayed confirmations during mode switch non-functional. Mode switch triggered during confirmation prompt leads to confusing behavior.

### Stack Operations

- **[medium]** `stack_operations.rs:109-169` -- Sequential `describe_change_set` calls with no rate limiting. Could hit AWS throttling on stacks with many changesets.

- **[low]** `stack_operations.rs:32-59` -- Comment claims "Start both API calls in parallel" but futures are awaited sequentially (no `tokio::join!`). Not a correctness issue but misleading comment and missed performance opportunity.

### Spawned Task Safety

- **[low]** `interactive.rs:387` -- `lock().unwrap()` inside spawned timing task. Panic on poisoned mutex would be silently lost (abort at line 414 does not inspect task result).

- **[low]** All other `tokio::spawn` calls in CFN handlers properly store `JoinHandle` and await with `??` error handling. No leaked tasks.

### Test Environment Safety

- **[low]** `loaders/env.rs:65,77,153,163` -- Test code uses `unsafe { set_var/remove_var }` in `#[tokio::test]`. Can race with parallel test execution. Test-only issue.

### Global State

- **[low]** `color.rs:8` -- `OnceLock<ColorContext>` pattern is correct. `global()` panics if not initialized; test isolation depends on `init_test_context()` being called.

- **[low]** `mod.rs:400-403` -- Poisoned mutex in `get_used_tokens` returns empty `vec![]` silently, potentially masking a prior panic.

- **[low]** `aws/mod.rs:115-117` -- Synchronous `exists()` in async context blocks tokio worker. Minor.

### Runtime Setup

No findings. Single `Runtime::new()` with `rt.block_on()` from main thread. No multi-runtime or nested-runtime issues.
