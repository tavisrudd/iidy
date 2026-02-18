# Handoff: docs/dev/ Prose Fixes from Cold Review

**Date**: 2026-02-18

## Done

### COVERAGE.md archived
- Moved `docs/dev/COVERAGE.md` to `notes/archive/COVERAGE.md`
- Removed from `notes/index.md`

### codebase-guide.md trimmed
- Removed `let*` jargon, now says "sequentially" and points to js-compatibility.md
- Tag listing replaced with 3 examples + "..." pointing to architecture.md
- Import types listing replaced with 3 examples + "..." pointing to architecture.md
- JS reference section (was ~70 lines) replaced with brief summary of key intentional diffs + pointer to js-compatibility.md and notes/
- Custom resource template detail removed (lives in notes/ and js-compat already)
- Removed absolute counts (test counts, file counts, line counts)
- Fixed `~2000 lines` reference on interactive.rs

### architecture.md trimmed
- Removed `let*` jargon, now says "sequentially" and points to js-compatibility.md
- PreprocessingTag listing trimmed to 3 examples + "..."
- Import type listing trimmed to 3 examples + "..."
- Removed absolute counts (test file counts, module counts, snapshot counts, "~400+ tests")
- Removed `~2000 lines` reference on InteractiveRenderer
- Removed specific coverage reference, just points to Makefile

### output-architecture.md
- Removed `~2000 lines` reference

### MD5 -> SHA256 for template approval
- `src/cfn/template_hash.rs`: Switched from `md5::compute` to `sha2::Sha256`
- Added doc comment explaining the function
- Fixed test assertion from 32 chars (MD5) to 64 chars (SHA256)
- Removed `md5` crate from `Cargo.toml` (no other uses)
- Fixed `docs/dev/codebase-guide.md` reference (was incorrectly "SHA256", then "MD5", now correctly "SHA256")
- Fixed `docs/dev/adr/003-template-approval.md` from "MD5" to "SHA256"

### let* semantics consolidated
- Only explained in `docs/dev/js-compatibility.md` (line 67-70)
- `architecture.md` and `codebase-guide.md` just say "sequentially" and cross-reference

## Remaining

### Verify build and tests pass
- `make check` passes (zero warnings)
- `make test` NOT yet run -- test run OOMed during linking (low memory)
- The SHA256 change is straightforward but the template_hash tests need to be verified
- The snapshot for any template-approval test might need updating if it includes hash values

### Potential snapshot updates
- If any insta snapshot captures a template hash value, it will change from 32 to 64 hex chars
- Search: `grep -r 'template_hash\|approval.*hash' tests/snapshots/`
- User must accept any snapshot changes

### Factual issues from the earlier code-checking review (not addressed here)
These were identified in the factual review but are separate from the prose fixes:
- `IidyError` enum referenced in codebase-guide.md line 56 does not exist (actual: `ErrorId`, `EnhancedPreprocessingError`)
- Missing files in codebase-guide directory trees (`detection.rs`, `location.rs`, `tree_sitter_location.rs`, `context.rs`, `enhanced.rs`, `ids.rs`)
- output-architecture.md has several factual errors (method names, variant names)
- aws-config.md missing credential source detection system
- See main conversation for the full factual review details
