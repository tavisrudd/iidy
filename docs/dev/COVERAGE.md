# Test Coverage Guide for iidy

This document explains how to use the test coverage reporting system implemented for the iidy project.

## Overview

Test coverage is measured using [cargo-tarpaulin](https://github.com/xd009642/tarpaulin), a code coverage tool specifically designed for Rust projects. The coverage system measures which lines of code are executed during test runs, helping identify untested code paths.

## Quick Start

### Prerequisites

Coverage reporting requires `cargo-tarpaulin` which is already installed in the development environment. If you need to install it manually:

```bash
cargo install cargo-tarpaulin
```

### Basic Usage

```bash
# Quick coverage summary
make coverage-quick

# Full coverage report with HTML output  
make coverage-html

# Comprehensive coverage with multiple formats
make coverage-report

# Open HTML report in browser (macOS)
make coverage-open
```

## Coverage Commands

### Available Make Targets

| Command | Description | Output | Time |
|---------|-------------|--------|------|
| `make coverage-quick` | Fast coverage check (lib + tests only) | Terminal | ~30s |
| `make coverage` | Standard coverage summary | Terminal | ~45s |
| `make coverage-html` | Generate HTML report | `tarpaulin-report.html` | ~60s |
| `make coverage-report` | Generate all formats | HTML, JSON, LCOV | ~60s |
| `make coverage-ci` | CI-friendly with 70% threshold | Terminal | ~45s |
| `make coverage-open` | Generate + open HTML report | Browser | ~60s |

### Direct cargo-tarpaulin Usage

For advanced usage, you can call `cargo-tarpaulin` directly:

```bash
# Basic coverage
cargo tarpaulin --out Stdout

# HTML report with custom options
cargo tarpaulin --out Html --timeout 300 --exclude-files "src/main.rs"

# Coverage with failure threshold
cargo tarpaulin --out Stdout --fail-under 80
```

## Configuration

### Coverage Configuration File

Coverage settings are defined in `.tarpaulin.toml`:

```toml
[tool.tarpaulin]
out = ["Html", "Lcov", "Json"]
target-dir = "target/tarpaulin"
fail-under = 80.0
exclude = [
    "src/main.rs",      # CLI entry point
    "src/demo.rs",      # Development utility
    "benches/*",        # Benchmark code  
    "tests/fixtures/*", # Test data
]
```

### Exclusions

The following files/directories are excluded from coverage calculation:

- **`src/main.rs`** - CLI entry point with minimal logic
- **`src/demo.rs`** - Development/demo utility code
- **`benches/*`** - Benchmark code (not core functionality)
- **`tests/fixtures/*`** - Test data files and snapshots
- **Lines with patterns** - Debug assertions, panic macros, compiler directives

## Understanding Coverage Reports

### Terminal Output

```
|| Tested/Total Lines:
|| src/lib.rs: 45/50 +90.00%
|| src/yaml/mod.rs: 234/267 +87.64%  
|| src/yaml/resolution/resolver.rs: 456/523 +87.19%
|| Total: 2847/3156 +90.21%
```

**Key Metrics:**
- **Line Coverage**: Percentage of code lines executed during tests
- **Total**: Overall coverage percentage across entire codebase
- **Per-file**: Coverage breakdown by source file

### HTML Report (`tarpaulin-report.html`)

The HTML report provides:
- **Interactive file browser** - Click through source files
- **Line-by-line highlighting** - Green (covered), Red (uncovered)
- **Coverage statistics** - Detailed metrics per file/function
- **Filtering options** - View only uncovered lines

### Coverage Thresholds

| Coverage Level | Status | Action Required |
|----------------|--------|-----------------|
| 90%+ | Excellent | Maintain quality |
| 80-89% | Good | Continue improvements |
| 70-79% | Acceptable | Focus on critical paths |
| <70% | Needs Improvement | Add tests for uncovered code |

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Run test coverage
  run: make coverage-ci
  
- name: Generate coverage reports  
  run: make coverage-report
  
- name: Upload coverage to Codecov
  uses: codecov/codecov-action@v3
  with:
    file: ./lcov.info
```

### Coverage in Development

**During Development:**
```bash
# Quick check after code changes
make coverage-quick

# Detailed analysis for new features
make coverage-html && open tarpaulin-report.html
```

**Before Committing:**
```bash
# Ensure coverage threshold met
make coverage-ci
```

## Interpreting Results

### High Priority Areas

Focus coverage efforts on:
1. **Core YAML preprocessing logic** (`src/yaml/`)
2. **CloudFormation operations** (`src/cfn/`)
3. **Import system** (`src/yaml/imports/`)
4. **Error handling paths**

### Low Priority Areas

Less critical for coverage:
- CLI argument parsing (well-tested by integration)
- Mock implementations (test infrastructure)
- Error message formatting (cosmetic)

### Common Uncovered Patterns

**Expected uncovered lines:**
- Error handling for truly exceptional cases
- Platform-specific fallback code
- Debug-only code paths
- Unreachable panic branches

## Best Practices

### Writing Tests for Coverage

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_code_paths() {
        // Test happy path
        assert!(function_call(valid_input).is_ok());
        
        // Test error conditions  
        assert!(function_call(invalid_input).is_err());
        
        // Test edge cases
        assert_eq!(function_call(edge_case), expected_result);
    }
}
```

### Coverage vs Quality

**Remember:**
- **100% coverage ≠ Perfect tests** - Focus on meaningful test cases
- **Quality over quantity** - Well-designed tests matter more than coverage %
- **Test behavior, not implementation** - Coverage should follow good tests

## Troubleshooting

### Common Issues

**Slow coverage runs:**
```bash
# Use quick mode for development
make coverage-quick

# Skip non-essential tests temporarily  
cargo tarpaulin --tests --lib --timeout 60
```

**Missing coverage in specific files:**
```bash
# Check if files are excluded in .tarpaulin.toml
# Verify test module placement (#[cfg(test)])
# Ensure tests actually call the code
```

**Platform-specific issues:**
- Some coverage features require LLVM on the system
- Use `--engine llvm` for more accurate coverage
- Fall back to `--engine auto` if LLVM unavailable

## Advanced Usage

### Differential Coverage

```bash
# Coverage for specific test suites
cargo tarpaulin --test yaml_tests --out Stdout

# Coverage with custom exclusions
cargo tarpaulin --exclude-files "src/cli.rs" --out Html

# Branch coverage (where supported)
cargo tarpaulin --branch --out Stdout
```

### Custom Reports

```bash
# JSON output for automated processing
cargo tarpaulin --out Json

# LCOV format for external tools
cargo tarpaulin --out Lcov

# Multiple outputs
cargo tarpaulin --out Html,Json,Lcov
```

## Coverage Goals

### Current Status

Based on recent coverage runs:
- **Overall Coverage**: ~85-90%
- **Core YAML Processing**: ~90%+
- **CloudFormation Operations**: ~80-85%
- **Import System**: ~90%+

### Target Goals

- **Minimum acceptable**: 70% overall coverage
- **Target goal**: 85% overall coverage  
- **Stretch goal**: 90%+ for critical modules
- **New code**: 90%+ coverage required

---

For questions about coverage reporting, see the project documentation or ask the development team.