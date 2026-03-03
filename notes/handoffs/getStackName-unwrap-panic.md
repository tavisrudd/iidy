# Fix `stack_name.as_ref().unwrap()` Panic Pattern

**Status:** t-later
**Severity:** Medium -- runtime panic on user-facing input error
**Filed:** 2026-03-02

## Problem

`StackArgs.stack_name` is `Option<String>` (see `src/cfn/stack_args.rs:15`).
Six call sites do `stack_args.stack_name.as_ref().unwrap()`, which panics with
an unhelpful message if StackName is missing from the YAML argsfile. A missing
StackName should produce a clear error like:

```
Error: StackName is required in stack-args.yaml
```

## Call Sites

All six unwrap sites:

| # | File                              | Line | Function                             |
|---|-----------------------------------|------|--------------------------------------|
| 1 | `src/cfn/update_stack.rs`         | 119  | `update_stack_with_changeset`        |
| 2 | `src/cfn/exec_changeset.rs`       | 128  | `perform_changeset_execution`        |
| 3 | `src/cfn/changeset_operations.rs` | 122  | `create_changeset_comprehensive`     |
| 4 | `src/cfn/changeset_operations.rs` | 345  | `build_changeset_result`             |
| 5 | `src/cfn/create_or_update.rs`     | 245  | `update_stack_with_changeset_data`   |
| 6 | `src/cfn/create_or_update.rs`     | 325  | `create_stack_with_changeset_data`   |

## Recommended Approach: Early Validation in `load_stack_args`

The simplest fix is to validate `stack_name` immediately after deserialization
in `load_stack_args()` (`src/cfn/stack_args.rs`, around line 235). Add:

```rust
// After: let mut stack_args: StackArgs = serde_yaml::from_value(final_value)?;
if stack_args.stack_name.is_none() {
    bail!("StackName is required in {argsfile}");
}
```

This ensures every downstream consumer can rely on `stack_name` being `Some`.
The six `.unwrap()` calls then become safe (though `.expect("validated in load_stack_args")`
would be clearer documentation).

### Alternative: Accessor method

Add a method to `StackArgs`:

```rust
impl StackArgs {
    pub fn stack_name(&self) -> Result<&str> {
        self.stack_name.as_deref()
            .ok_or_else(|| anyhow!("StackName is required but missing from stack args"))
    }
}
```

Replace all six `.as_ref().unwrap()` calls with `.stack_name()?`. This is more
defensive (protects against future callers that skip `load_stack_args`) but
touches more files.

### Haskell Port Reference

The Haskell port (`iidy-hs`) has `getStackName :: StackArgs -> Text` which
falls back to `"unnamed-stack"` via `fromMaybe`. This avoids a panic but
silently proceeds with a bogus name. The Rust fix should prefer an explicit
error over a silent fallback.

## Files to Modify

| File                              | Change                                                       |
|-----------------------------------|--------------------------------------------------------------|
| `src/cfn/stack_args.rs`           | Add validation after line 235, or add accessor method        |
| `src/cfn/update_stack.rs`         | Replace `.unwrap()` with `.expect()` or `stack_name()?`      |
| `src/cfn/exec_changeset.rs`       | Replace `.unwrap()` with `.expect()` or `stack_name()?`      |
| `src/cfn/changeset_operations.rs` | Replace 2x `.unwrap()` with `.expect()` or `stack_name()?`   |
| `src/cfn/create_or_update.rs`     | Replace 2x `.unwrap()` with `.expect()` or `stack_name()?`   |

## Tests

1. **Unit test in `stack_args.rs`:** Deserialize YAML without StackName, assert `load_stack_args` returns an error containing "StackName is required".
2. **Existing tests:** Verify all existing tests still pass (they all include StackName so should be unaffected).
3. **Integration smoke test:** If you have a CLI integration test harness, run a `create-stack` with an argsfile that omits StackName and verify the error message.

## Notes

- The `CommandsBefore` processing path in `process_commands_before` (line 435)
  already handles `stack_name` safely via `if let Some(stack_name) = ...`. No
  change needed there.
- No `unwrap_or` variants exist -- the six `.unwrap()` calls listed above are
  the complete set.
