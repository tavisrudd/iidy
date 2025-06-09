# Security Model for YAML Import System

This document describes the security model implemented in iidy's YAML preprocessing import system to prevent malicious remote templates from accessing local resources.

## Overview

The import system distinguishes between **local templates** (files loaded from the local filesystem) and **remote templates** (files loaded from S3, HTTP, or HTTPS URLs), applying different security restrictions to each.

## Local vs Remote Templates

### Local Templates
- **Definition**: Files loaded from the local filesystem (no URL scheme)
- **Examples**: `config.yaml`, `./app.yaml`, `/absolute/path/config.yaml`
- **Security**: No restrictions - can import from any source type
- **Rationale**: Already executing in a trusted local context

### Remote Templates  
- **Definition**: Files loaded from S3, HTTP, or HTTPS URLs
- **Examples**: `s3://bucket/config.yaml`, `https://example.com/config.yaml`
- **Security**: Subject to restrictions to prevent local resource access
- **Rationale**: Untrusted external content should not access local resources

## Import Type Security Classification

### 🚫 Local-Only Import Types (Forbidden from Remote Templates)

| Import Type | Description | Security Risk |
|-------------|-------------|---------------|
| `file:` | Local filesystem access | Could read sensitive files |
| `env:` | Local environment variables | Could access secrets/credentials |
| `git:` | Local git repository access | Could extract repository information |
| `filehash:` | File hashing (typically local files) | Could scan filesystem structure |
| `filehash-base64:` | File hashing with base64 encoding | Could scan filesystem structure |

### ✅ Remote-Allowed Import Types

| Import Type | Description | Use Case |
|-------------|-------------|-----------|
| `s3:` | S3 objects | Access other S3-stored configurations |
| `http:`/`https:` | HTTP endpoints | Fetch configuration from web APIs |
| `cfn:` | CloudFormation stacks and exports | Dynamic AWS resource references |
| `ssm:` | SSM parameters | Centralized configuration management |
| `ssm-path:` | SSM parameter paths | Bulk parameter retrieval |
| `random:` | Random value generation | Generate unique identifiers |

## Relative Import Behavior

### Inheritance from Parent Template

When a template uses relative imports (no explicit type prefix), the import inherits the type of the parent template:

```yaml
# From s3://bucket/configs/app.yaml
$imports:
  database: "database.yaml"  # ✅ Resolves to s3://bucket/configs/database.yaml (inherits S3)
```

### Local Path Indicators Blocked

Explicit local path indicators are forbidden from remote templates:

```yaml
# From s3://bucket/config.yaml - These will be REJECTED:
$imports:
  bad1: "./local.yaml"      # ❌ Error: local path from remote
  bad2: "../local.yaml"     # ❌ Error: local path from remote  
  bad3: "/abs/local.yaml"   # ❌ Error: local path from remote
```

## Examples

### ✅ Allowed: Remote-to-Remote Imports

```yaml
# From https://example.com/configs/app.yaml
$imports:
  s3data: "s3://bucket/data.yaml"           # ✅ S3 import
  webdata: "https://api.com/config.json"    # ✅ HTTP import  
  database: "database.yaml"                 # ✅ Relative (inherits HTTPS)
  cfnstack: "cfn:stack/MyStack/output"      # ✅ CloudFormation
  secret: "ssm:/app/secret"                 # ✅ SSM parameter
  uuid: "random:dashed-name"                # ✅ Random generation
```

### ❌ Forbidden: Remote-to-Local Imports

```yaml
# From s3://bucket/config.yaml - All of these are REJECTED:
$imports:
  localfile: "file:./local.yaml"      # ❌ File access
  envvar: "env:HOME"                  # ❌ Environment variable
  gitinfo: "git:branch"               # ❌ Git repository
  hash: "filehash:./data.txt"         # ❌ Local file hash
  localpath: "./local.yaml"           # ❌ Local path indicator
```

### ✅ Allowed: Local Template Flexibility

```yaml
# From local file ./config.yaml - All imports allowed:
$imports:
  localfile: "file:./other.yaml"      # ✅ Local file
  envvar: "env:HOME"                  # ✅ Environment variable
  s3data: "s3://bucket/data.yaml"     # ✅ S3 object
  webdata: "https://api.com/data"     # ✅ HTTP endpoint
  gitbranch: "git:branch"             # ✅ Git info
  hash: "filehash:./data.txt"         # ✅ File hash
```

## Base Path Resolution for Relative Imports

The system derives base paths to enable relative imports across different contexts:

### Local File Paths
- `/Users/app/configs/main.yaml` → `/Users/app/configs/`
- `./configs/app.yaml` → `./configs/`
- `config.yaml` → `` (empty - current directory)

### S3 URLs
- `s3://bucket/file.yaml` → `s3://bucket/`
- `s3://bucket/configs/app.yaml` → `s3://bucket/configs/`
- `s3://bucket/configs/env/prod.yaml` → `s3://bucket/configs/env/`

### HTTP/HTTPS URLs
- `https://example.com/file.yaml` → `https://example.com/`
- `https://example.com/configs/app.yaml` → `https://example.com/configs/`
- `http://api.com/templates/base.yaml` → `http://api.com/templates/`

## Threat Model

### What This Protects Against

1. **Credential Theft**: Prevents remote templates from reading environment variables containing secrets
2. **File System Scanning**: Blocks access to local files and directory structures
3. **Information Disclosure**: Prevents extraction of git repository information
4. **Privilege Escalation**: Stops remote templates from accessing local resources beyond their intended scope

### What This Allows

1. **Legitimate Composition**: Remote templates can compose from other remote sources
2. **Relative Imports**: Templates can reference related files in the same remote context
3. **AWS Integration**: Remote templates can access CloudFormation and SSM for dynamic configuration
4. **Local Flexibility**: Local templates retain full import capabilities for trusted operations

## Error Messages

When a security violation is detected, the system returns a clear error message:

```
Import type 'file:./local.yaml' in 's3://bucket/config.yaml' not allowed from remote template
```

This helps developers understand why their import was rejected and how to fix it.

## Implementation Notes

- Security validation occurs in the `parse_import_type()` function in `src/yaml/imports/mod.rs`
- Base path derivation happens in `TagContext::from_processing_env()` in `src/yaml/resolution/resolver.rs`
- The security model is extensively tested with over 150 test cases covering various scenarios
- The implementation follows the principle of "secure by default" - remote templates are restricted unless explicitly allowed