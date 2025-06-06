# iidy Example Templates

This directory contains example templates demonstrating iidy's YAML preprocessing capabilities, CloudFormation integration, and template composition features.

## Organization

### Root Level Templates
- **basic-test.yaml** - Simple template demonstrating variable substitution and handlebars
- **config.yaml** - Configuration template showing structured data and environment settings
- **import-test.yaml** - Demonstrates `$imports` functionality for template composition
- **simple-cloudformation.yaml** - Basic CloudFormation template with iidy preprocessing
- **advanced-cloudformation.yaml** - Complex multi-tier AWS infrastructure template
- **cloudformation-tags-demo.yaml** - Comprehensive demonstration of CloudFormation tags with preprocessing

### Subdirectories

- **invalid/** - Templates with syntax errors or unimplemented features (used for error handling tests)
- **expected-outputs/** - Reference output files for manual verification

## Auto-Testing

**All templates in this directory are automatically tested!**

The test suite (`tests/example_templates_snapshots.rs`) includes:

### Automatic Discovery
- **Recursive scanning**: All `.yaml` files in this directory and subdirectories are automatically discovered
- **Snapshot testing**: Each template's output is captured as a snapshot using `insta`
- **Regression protection**: Any change to template output will cause tests to fail
- **Path-aware naming**: Subdirectory templates get descriptive snapshot names (e.g., `aws/s3.yaml` → `auto_discovered_aws_s3`)

### Excluded Directories
- `invalid/` - Templates expected to fail (tested separately)
- `expected-outputs/` - Reference files (not processed)
- `.git/` - Version control files
- Hidden files starting with `.`

### Adding New Examples

To add a new example template:

1. **Create the file**: Place your `.yaml` template anywhere in this directory structure
2. **Run tests**: `cargo test --test example_templates_snapshots`
3. **Review snapshots**: `cargo insta review` to accept new snapshots
4. **Commit**: Include both your template and the generated snapshot

**No manual test configuration required!** The auto-discovery system will automatically:
- Find your new template
- Generate a snapshot of its processed output
- Protect against future regressions

### Organizing Examples

You can organize examples in subdirectories by category:

```
example-templates/
├── aws/                    # AWS-specific examples
│   ├── s3-bucket.yaml
│   └── lambda.yaml
├── cloudformation/         # CloudFormation patterns
│   ├── vpc-setup.yaml
│   └── security-groups.yaml
├── handlebars/            # Handlebars features
│   ├── conditionals.yaml
│   └── loops.yaml
└── imports/               # Import examples
    ├── nested-imports.yaml
    └── env-configs.yaml
```

All subdirectories will be automatically scanned and tested.

## Template Features Demonstrated

### Core Preprocessing
- **Variable definitions** (`$defs`)
- **Handlebars templating** (`{{variable}}`)
- **Template imports** (`$imports`)
- **Conditional logic** (`!$if`, `!$eq`)

### CloudFormation Integration
- **Tag preservation** - CloudFormation intrinsic functions (`!Ref`, `!Sub`, `!GetAtt`) are preserved
- **Nested tags** - Complex combinations like `!Base64` with `!Sub`
- **Mixed preprocessing** - Handlebars variables processed within CloudFormation tags
- **YAML 1.1 compatibility** - Boolean handling for CloudFormation

### Advanced Features
- **Import resolution** - Loading external configuration files
- **Environment-based configuration** - Different settings per deployment environment
- **Complex data structures** - Nested mappings and arrays
- **Multi-file composition** - Building templates from multiple sources

## Running Tests

```bash
# Run all example template tests
cargo test --test example_templates_snapshots

# Run just the auto-discovery test
cargo test test_all_example_templates_auto_discovery

# Review and accept new snapshots
cargo insta review

# Accept all pending snapshots
cargo insta accept
```

## Snapshot Files

Generated snapshots are stored in `tests/snapshots/` and should be committed to version control. They serve as:
- **Regression tests** - Detect unexpected changes in output
- **Documentation** - Show exactly what each template produces
- **Verification** - Ensure consistent behavior across code changes

The snapshot testing ensures that all examples remain working and any changes to iidy's preprocessing behavior are explicitly reviewed and approved.