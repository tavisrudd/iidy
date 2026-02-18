# Renderer CLI Context Analysis: Requiring CLI for Construction

**Date:** 2025-09-28
**Status:** Investigation Complete

## Problem

Currently, `InteractiveRenderer` uses `self.cli_context: Option<Arc<Cli>>` which can be `None` in tests. In all production command handlers we always have Cli available. Having this an Option needlessly complicates the code in `InteractiveRenderer`. We want to make it a required argument to the j

## Investigation: CLI Availability in Production

### DynamicOutputManager Creation Patterns

**1. `run_command_handler!` macro (most commands):**
- Location: `src/cfn/mod.rs` lines 23-30
- Pattern: `OutputOptions::new($cli.clone())` → `DynamicOutputManager::new()`
- CLI: ✅ Always available (passed as macro parameter)

**2. Manual creation (2 commands only):**
- `get_stack_template(cli: &Cli, args: &GetTemplateArgs)`
- `get_stack_instances(cli: &Cli, args: &GetStackInstancesArgs)`
- Pattern: `OutputOptions::new(cli.clone())` → `DynamicOutputManager::new()`
- CLI: ✅ Always available (function parameter)

### Key Finding

**✅ CLI is ALWAYS available during renderer construction in non-test scenarios**

1. All CFN command entry points receive `cli: &Cli` parameter
2. All renderer construction happens within command functions
3. Zero scenarios found where renderers created without CLI context

### Test Scenarios (where CLI unavailable)

- `InteractiveOptions::default()` → `cli_context: None`
- `InteractiveOptions::plain()` → `cli_context: None`
- Test helpers explicitly set `cli_context: None`

## Recommendation: Require CLI for Construction

### Benefits
1. **Eliminates inconsistency** - no more mixing `self.cli_context` vs `cli` parameter
2. **Production guarantee** - CLI always available when needed
3. **Cleaner setup** - section ordering configured correctly during construction

### Implementation
1. **Require CLI in `InteractiveOptions`** for production use
2. **Keep `self.cli_context: Option<Arc<Cli>>`** for test compatibility
3. **Use CLI parameter** for all section ordering logic
4. **Fallback to `self.cli_context`** for remaining methods (metadata, context info)

### Current `self.cli_context` Usage
- `render_command_metadata()` - get CFN operation
- `get_stack_absent_context_info()` - get environment/region

These can continue using existing pattern for test compatibility.

## Conclusion

**Feasible and recommended.** CLI is always available in production, tests can continue with current patterns.
