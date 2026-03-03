# Task: Unknown-Key Validation in stack-args.yaml Parser

**Created**: 2026-03-02
**Status**: Ready for implementation
**Estimated effort**: 1-2 sessions (~2 commits)
**Cross-reference**: Haskell port (iidy-hs) has this implemented already

## Problem

Unknown top-level keys in `stack-args.yaml` are silently ignored by serde
deserialization. If a user writes `Paramters` instead of `Parameters`, the key
is silently dropped and the parameters are never sent to CloudFormation. The
stack operation proceeds without error, but the user's configuration is
partially missing.

This is a high-severity UX issue. A one-character typo in a key name can cause:
- Stack creation with missing parameters (CloudFormation then fails with a
  confusing error about missing parameter values)
- Missing tags, capabilities, notification ARNs, etc.
- No indication that anything in the YAML was wrong

## Current Behavior

The `StackArgs` struct in `src/cfn/stack_args.rs` (line 13) uses serde
`Deserialize` with `#[serde(rename = "...")]` for each field. Serde's default
behavior is to silently ignore unknown fields. There is no
`#[serde(deny_unknown_fields)]` attribute.

The deserialization happens at two points in `load_stack_args()`:
- Line 202: `serde_yaml::from_value(pass1_value)?` (first pass for CommandsBefore)
- Line 235: `serde_yaml::from_value(final_value)?` (final deserialization)

## Approach Options

### Option A: `#[serde(deny_unknown_fields)]`

Add the attribute to the StackArgs struct:

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct StackArgs { ... }
```

**Pros**: One-line change.
**Cons**:
- Serde's error messages are generic: `unknown field "Paramters", expected one of ...`
  followed by all 20+ field names. No "did you mean?" suggestion.
- **Breaks $envValues handling**: `$envValues` is injected into the YAML mapping
  before preprocessing but is NOT a field in StackArgs. With `deny_unknown_fields`,
  any residual `$envValues` key would cause a hard error. The preprocessing
  pipeline strips `$imports`, `$defs`, `$envValues`, and `$params` during
  resolution, but `$envValues` is injected *after* the initial preprocessing
  pass in `load_stack_args()` (line 187), then a second preprocessing pass
  happens (line 231-232). If `$envValues` survives preprocessing, serde will
  reject it.
- Cannot be used for the first-pass deserialization (line 202) where
  `$envValues` and other preprocessor keys may still be present.

**Verdict**: Not recommended. Too fragile and poor error messages.

### Option B: Post-deserialization key validation with edit-distance suggestions

Validate keys by inspecting the raw `serde_yaml::Value::Mapping` *before*
passing it to `serde_yaml::from_value`. Compare the set of keys against the
known valid set. For unknown keys, compute Levenshtein distance to find the
best "did you mean?" suggestion.

**Pros**:
- Full control over error messages
- "Did you mean X?" suggestions for typos
- Can exclude internal keys (`$envValues`, `$imports`, `$defs`, `$params`)
  from validation and from suggestions
- Matches what the Haskell port does (consistency)
- No risk of breaking the preprocessor pipeline

**Cons**: More code than Option A (~60 lines).

**Verdict**: Recommended. This is what iidy-hs implements and it provides
much better UX.

## Implementation Details

### Valid Key Set (21 user-facing keys + internal keys)

These are the valid top-level keys, derived from the `StackArgs` struct
`#[serde(rename = "...")]` attributes:

```
StackName
Template
ApprovedTemplateLocation
Region
Profile
AssumeRoleARN
ServiceRoleARN
RoleARN
Capabilities
Tags
Parameters
NotificationARNs
TimeoutInMinutes
OnFailure
DisableRollback
EnableTerminationProtection
StackPolicy
ResourceTypes
UsePreviousTemplate
UsePreviousParameterValues
CommandsBefore
```

Internal keys that should be **accepted but not suggested**:
```
$envValues
$imports
$defs
$params
```

These are YAML preprocessor keys that may or may not be fully stripped before
the validation point. They should be silently accepted (not flagged as
unknown) but never suggested as corrections for typos.

### Levenshtein Distance

The codebase already has a hand-rolled Levenshtein distance implementation in
`src/yaml/errors/enhanced.rs` (line 650, function `levenshtein_distance`).
The `strsim` crate is also already in `Cargo.lock` (pulled in by `clap`), so
either approach works:

1. **Reuse the existing `levenshtein_distance`** from `enhanced.rs` -- move it
   to a shared utility module, or make it `pub`.
2. **Add `strsim` as a direct dependency** and use `strsim::levenshtein`.
3. **Copy the implementation** into `stack_args.rs` (least preferred -- code
   duplication).

Recommendation: Extract the existing implementation into a shared module
(e.g., `src/util.rs` or `src/string_utils.rs`) and use it from both
`enhanced.rs` and `stack_args.rs`.

### Suggestion Threshold

The Haskell implementation uses: `max_dist = min(3, key_len / 2 + 1)`.
This means:
- For a 1-char key: max distance 1
- For a 4-char key: max distance 3
- For a 10-char key: max distance 3 (capped)
- Distance must be > 0 (don't suggest exact matches)

### Error Format

```
Unknown keys in stack-args: Paramters (did you mean Parameters?), StakName (did you mean StackName?)
```

For keys with no close match:
```
Unknown keys in stack-args: FooBarBazQux
```

Multiple unknown keys are comma-separated in a single error message.

### Where to Add Validation

Add a `validate_stack_args_keys` function in `src/cfn/stack_args.rs` that:

1. Takes a `&serde_yaml::Value` (the mapping about to be deserialized)
2. Extracts all top-level keys from the mapping
3. Filters out known valid keys and internal (`$`-prefixed) keys
4. For each unknown key, finds the best suggestion via edit distance
5. Returns `Result<()>` -- `Ok(())` if no unknown keys, `Err(anyhow!(...))` if any

Call this function in `load_stack_args()` **before** the `serde_yaml::from_value`
call at line 235. The validation should happen on `final_value` (after all
preprocessing is complete). Do NOT add it before the first-pass deserialization
at line 202, since that pass is just for CommandsBefore context and may have
more preprocessor keys present.

```rust
// Add before line 235:
validate_stack_args_keys(&final_value)?;

// Then the existing:
let mut stack_args: StackArgs = serde_yaml::from_value(final_value)?;
```

### Suggested Implementation

```rust
use std::collections::HashSet;

/// The 21 user-facing valid top-level keys in stack-args.yaml
const VALID_STACK_ARGS_KEYS: &[&str] = &[
    "StackName", "Template", "ApprovedTemplateLocation",
    "Region", "Profile", "AssumeRoleARN", "ServiceRoleARN", "RoleARN",
    "Capabilities", "Tags", "Parameters", "NotificationARNs",
    "TimeoutInMinutes", "OnFailure", "DisableRollback",
    "EnableTerminationProtection", "StackPolicy", "ResourceTypes",
    "UsePreviousTemplate", "UsePreviousParameterValues",
    "CommandsBefore",
];

/// Validate that a YAML mapping contains no unknown top-level keys.
/// Internal keys ($envValues, $imports, $defs, $params) are silently accepted.
fn validate_stack_args_keys(value: &Value) -> Result<()> {
    let mapping = match value {
        Value::Mapping(m) => m,
        _ => return Ok(()), // Not a mapping -- serde will handle the error
    };

    let valid: HashSet<&str> = VALID_STACK_ARGS_KEYS.iter().copied().collect();
    let mut unknown_keys: Vec<String> = Vec::new();

    for (key, _) in mapping.iter() {
        if let Value::String(key_str) = key {
            // Skip internal preprocessor keys
            if key_str.starts_with('$') {
                continue;
            }
            if !valid.contains(key_str.as_str()) {
                unknown_keys.push(key_str.clone());
            }
        }
    }

    if unknown_keys.is_empty() {
        return Ok(());
    }

    let formatted: Vec<String> = unknown_keys
        .iter()
        .map(|key| format_unknown_key(key, &valid))
        .collect();

    bail!("Unknown keys in stack-args: {}", formatted.join(", "))
}

fn format_unknown_key(key: &str, valid_keys: &HashSet<&str>) -> String {
    let max_dist = 3.min(key.len() / 2 + 1);
    let mut best: Option<(&str, usize)> = None;

    for &candidate in valid_keys {
        let dist = levenshtein_distance(key, candidate);
        if dist > 0 && dist <= max_dist {
            match best {
                None => best = Some((candidate, dist)),
                Some((_, best_dist)) if dist < best_dist => best = Some((candidate, dist)),
                _ => {}
            }
        }
    }

    match best {
        Some((suggestion, _)) => format!("{key} (did you mean {suggestion}?)"),
        None => key.to_string(),
    }
}
```

## Files to Modify

| File                           | Change                                                       |
|:-------------------------------|:-------------------------------------------------------------|
| `src/cfn/stack_args.rs`        | Add `validate_stack_args_keys`, call it before `from_value`  |
| `src/yaml/errors/enhanced.rs`  | Make `levenshtein_distance` pub, or extract to shared module |

If extracting to a shared module:

| File                           | Change                                             |
|:-------------------------------|:---------------------------------------------------|
| `src/util.rs` (new)            | `pub fn levenshtein_distance(s1: &str, s2: &str)`  |
| `src/lib.rs`                   | Add `pub mod util;`                                 |
| `src/yaml/errors/enhanced.rs`  | Use `crate::util::levenshtein_distance`             |
| `src/cfn/stack_args.rs`        | Use `crate::util::levenshtein_distance`             |

## Testing

### Unit tests to add in `src/cfn/stack_args.rs` (mod tests)

1. **Typo detection with suggestion**: Parse YAML with `Paramters` key, verify
   error contains "Paramters" and "did you mean Parameters?"

2. **Far-off key has no suggestion**: Parse YAML with `FooBarBazQux`, verify
   error contains the key name but NOT "did you mean"

3. **All valid keys pass**: Parse YAML with a subset of valid keys, verify
   no error

4. **$envValues is not flagged**: Parse YAML with `$envValues` key present,
   verify no error

5. **Multiple unknown keys**: Parse YAML with two typos, verify both appear
   in the error message

6. **Integration test**: Use `load_stack_args` with a temp file containing
   a typo, verify the error propagates through the full loading path

### Example test

```rust
#[test]
fn test_validate_stack_args_keys_typo() {
    let yaml = "StackName: test\nParamters: {}\n";
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let result = validate_stack_args_keys(&value);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Paramters"));
    assert!(err.contains("did you mean Parameters?"));
}

#[test]
fn test_validate_stack_args_keys_all_valid() {
    let yaml = "StackName: test\nTemplate: t.yaml\nParameters: {}\nTags: {}\n";
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    assert!(validate_stack_args_keys(&value).is_ok());
}

#[test]
fn test_validate_stack_args_keys_env_values_accepted() {
    let yaml = "StackName: test\n$envValues: {}\n";
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    assert!(validate_stack_args_keys(&value).is_ok());
}

#[test]
fn test_validate_stack_args_keys_far_off() {
    let yaml = "StackName: test\nFooBarBazQux: hi\n";
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let result = validate_stack_args_keys(&value);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("FooBarBazQux"));
    assert!(!err.contains("did you mean"));
}
```

## Notes for Implementor

- The `$envValues` key is injected by `inject_env_values()` at line 187 of
  `stack_args.rs` and may survive into the final YAML value. It is NOT a field
  in the StackArgs struct. Serde currently silently ignores it. Your validation
  must also silently accept it (and any other `$`-prefixed preprocessor key).

- The Haskell port's implementation is in
  `~/src/iidy-hs/src/Iidy/Cfn/StackArgsLoader.hs` lines 233-286. The tests
  are in `~/src/iidy-hs/test/Test/StackArgsLoaderTest.hs` lines 257-302.
  These are useful as a reference for exact behavior and edge cases.

- The existing `levenshtein_distance` in `src/yaml/errors/enhanced.rs` (line
  650) is a standard matrix implementation. It's currently private to that
  module. Either make it `pub` or extract it.

- The `strsim` crate is already in `Cargo.lock` (dependency of `clap`) but is
  NOT a direct dependency in `Cargo.toml`. If you want to use it directly,
  add `strsim = "0.11"` to `[dependencies]`. But reusing the existing
  implementation is cleaner.

- The `fuzzy_match_variables` function in `enhanced.rs` (line 630) uses
  threshold `max(1, max(len1, len2) / 3)`. The Haskell port uses
  `min(3, len / 2 + 1)`. Either threshold is reasonable; the Haskell one
  is slightly more conservative. Pick one and be consistent.
