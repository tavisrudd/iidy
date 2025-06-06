# Phase 1: Core YAML Preprocessing System

## Overview

This document tracks the design and implementation of the core YAML preprocessing system for iidy Rust, establishing the foundation for custom tags, imports, and template processing that provides feature parity with the TypeScript implementation.

## Requirements Analysis

### Core YAML Features to Implement

Based on iidy-js architecture, TypeScript implementation review, and YAML preprocessing documentation:

#### 1. Custom Tag System
- **Base Tag Architecture**: Generic `Tag` trait/struct for scalar, mapping, and sequence YAML nodes
- **Tag Registration**: System to register custom tags (equivalent to `addTagType()`)
- **Runtime Type Safety**: Type-safe tag creation and manipulation
- **Tag Visitor Pattern**: Modular processing system similar to iidy-js `Visitor` class

#### 2. Preprocessing Language Tags

**Data Import/Definition**:
- `$imports`: Import data from external files/sources (file, env, git, s3, http, ssm, cfn, etc.)
- `$defs`: Define local variables within document

**Logical Operations**:
- `!$if`: Conditional branching with test/then/else structure
- `!$eq`: Equality comparison between two values
- `!$not`: Boolean negation

**Data Transformation**:
- `!$map`: Transform lists/arrays with template and optional filtering
- `!$merge`: Combine mappings (like lodash merge)
- `!$concat`: Merge sequences/arrays
- `!$split`: String to array conversion with delimiter
- `!$join`: Array to string conversion with delimiter
- `!$let`: Local variable binding with scoped environment

**Advanced Transformations**:
- `!$concatMap`: Map followed by concat
- `!$mergeMap`: Map followed by merge
- `!$mapListToHash`: Convert list of key-value pairs to hash
- `!$mapValues`: Transform object values while preserving keys
- `!$groupBy`: Group items by key (like lodash groupBy)
- `!$fromPairs`: Convert key-value pairs to object

**String Processing**:
- `!$string`/`!$toYamlString`: Convert data to YAML string
- `!$parseYaml`: Parse YAML string back to data
- `!$toJsonString`: Convert data to JSON string
- `!$parseJson`: Parse JSON string back to data
- `!$escape`: Prevent preprocessing on child tree

#### 3. Handlebars-style String Processing
- Template variable substitution: `{{variable}}`
- String helpers: `toLowerCase`, `toUpperCase`, `base64`
- Data conversion: `toJson`, `toYaml`, `toJsonPretty`
- Support for handlebars-helpers string functions

#### 4. Import System
From TypeScript analysis, supports multiple import types:
- `file:` - Local files (YAML/JSON)
- `env:` - Environment variables
- `git:` - Git information (branch, describe, sha)
- `s3:` - S3 objects
- `http:`/`https:` - HTTP resources
- `ssm:` - AWS Systems Manager parameters
- `ssm-path:` - AWS Systems Manager parameter paths
- `cfn:` - CloudFormation stack outputs/exports
- `random:` - Random values (names, integers)
- `filehash:`/`filehash-base64:` - File content hashes

## Architecture Analysis from TypeScript

### Key Components from iidy-js

1. **Import Resolution System** (`src/preprocess/index.ts:452-469`):
   - Async import loading with type detection
   - Recursive import resolution for nested documents
   - SHA256 digest tracking for cache/metadata

2. **Visitor Pattern** (`src/preprocess/visitor.ts:108-855`):
   - Stateless visitor functions wrapped in extensible class
   - Separate visit methods for each tag type
   - Environment management with stack frames for error reporting
   - Reference rewriting for CloudFormation prefixing

3. **Environment Management**:
   - Hierarchical `$envValues` with import/def scoping
   - Stack frame tracking for error context
   - Global accumulator pattern for CloudFormation sections

4. **Filter System** (`src/preprocess/filter.ts`):
   - Variable dependency tracking
   - Selective import/def preservation

## Implementation Strategy

### YAML Parser Selection

**Recommended: serde_yml**
- More advanced than `serde_yaml` with better custom tag support
- Native support for `!tag` syntax and enum serialization
- Provides `singleton_map` modules for flexible tag handling
- Better suited for iidy's custom tag requirements

### Architecture Design

#### Core Module Structure
```
src/yaml/
├── mod.rs              # Main preprocessing entry point
├── ast.rs              # YAML AST types and Tag trait
├── tags.rs             # Custom tag implementations
├── parser.rs           # YAML parsing with custom schema
├── imports/
│   ├── mod.rs          # Import resolution system
│   └── loaders/        # Individual import type loaders
│       ├── file.rs
│       ├── env.rs
│       ├── git.rs
│       ├── http.rs
│       ├── random.rs
│       └── utils.rs
└── handlebars/
    ├── mod.rs          # Handlebars engine
    ├── engine.rs       # Template compilation/execution
    └── helpers/        # String helper functions
        ├── string_manip.rs
        ├── encoding.rs
        └── serialization.rs
```

#### Tag Processing Pipeline
Based on detailed analysis of the iidy-js implementation, the processing happens in two distinct phases:

**Phase 1 - Import Loading and Environment Building:**
1. **Parse Phase**: Load YAML with custom schema recognition
2. **$defs Processing**: Copy `$defs` values to `$envValues` (unprocessed, raw values)
3. **Import Resolution**: For each `$imports` entry:
   - Apply handlebars interpolation to import locations using current `$envValues`
   - Load imports from resolved locations (file, HTTP, env, etc.)
   - Add imported data to `$envValues`
   - Recursively process nested imports in imported documents
4. **Environment Complete**: Full `$envValues` environment is built with all imports and defs

**Phase 2 - Tag Processing and Final Resolution:**
1. **Custom Tag Processing**: Process all `!$` tags using visitor pattern with complete environment
2. **Handlebars Interpolation**: Apply template processing to string values
3. **Include Resolution**: Resolve `!$` includes with dot notation access to environment
4. **Output Generation**: Generate final processed YAML/JSON

**Key Insight**: Handlebars templating happens in **both phases**:
- Phase 1: Dynamic import paths (e.g., `"{{environment}}-config.yaml"`)
- Phase 2: Final template variable resolution in output values

#### Environment Management Pattern (from TypeScript)
```rust
pub struct Env {
    pub global_accumulator: CfnDoc,
    pub env_values: HashMap<String, Value>,
    pub stack: Vec<StackFrame>,
}

pub struct StackFrame {
    pub location: String,
    pub path: String,
}
```

### Rust-Specific Implementation Considerations

- **Error Handling**: Use `anyhow` for error chain propagation with stack frame context
- **Async Runtime**: Leverage existing tokio patterns for import loading
- **Type Safety**: Use `serde` traits for type-safe deserialization after preprocessing
- **Integration**: Seamlessly integrate with existing `CfnContext` and AWS patterns
- **Testing**: Maintain deterministic offline testing approach with fixture-based imports

## Current State Analysis

### ✅ Already Implemented
Based on codebase research, the following components are already in place:

**Core Infrastructure:**
- ✅ Custom YAML AST types (`src/yaml/ast.rs`) with preprocessing tag support
- ✅ YAML parser with custom tag recognition (`src/yaml/parser.rs`)
- ✅ Tag resolution framework with TagContext (`src/yaml/tags.rs`)
- ✅ AstResolver trait for modular tag processing
- ✅ Handlebars template engine (`src/yaml/handlebars/`)
- ✅ Import system framework (`src/yaml/imports/`)

**Core Tags Implemented:**
- ✅ `!$` / `!$include` - Include content (basic structure)
- ✅ `!$if` - Conditional logic with test/then/else
- ✅ `!$map` - Transform lists/arrays with variable binding
- ✅ `!$merge` - Combine mappings/objects
- ✅ `!$concat` - Merge sequences/arrays
- ✅ `!$let` - Variable binding with scoped context
- ✅ `!$eq` - Equality comparison
- ✅ `!$not` - Boolean negation
- ✅ `!$split` - String to array conversion
- ✅ `!$join` - Array to string conversion

**Infrastructure Components:**
- ✅ TagContext with variable scoping and base path resolution
- ✅ Integration with stack_args.rs preprocessing
- ✅ Basic test coverage and AST conversion utilities
- ✅ Value comparison utilities for equality operations

### ❌ Gaps Identified

**Import System Loaders:**
- ❌ File loader implementation (stubs exist in `src/yaml/imports/loaders/`)
- ❌ Environment variable loader
- ❌ Git information loader (branch, sha, describe)
- ❌ HTTP/HTTPS loader
- ❌ Random value generator
- ❌ AWS loaders (S3, SSM, CloudFormation)
- ❌ Filehash computation

**Advanced Tags:**
- ✅ `!$concatMap` - Map followed by concat
- ✅ `!$mergeMap` - Map followed by merge
- ✅ `!$mapListToHash` - Convert list of key-value pairs to hash
- ✅ `!$mapValues` - Transform object values while preserving keys
- ✅ `!$groupBy` - Group items by key (like lodash groupBy)
- ✅ `!$fromPairs` - Convert key-value pairs to object
- ✅ `!$toYamlString` - Convert data to YAML string
- ✅ `!$parseYaml` - Parse YAML string back to data
- ✅ `!$toJsonString` - Convert data to JSON string
- ✅ `!$parseJson` - Parse JSON string back to data
- ✅ `!$escape` - Prevent preprocessing on child tree

**Special Processing:**
- ✅ `$imports` and `$defs` key processing in mappings (Phase 1)
- ✅ Handlebars interpolation in import locations during Phase 1
- ✅ Two-phase processing pipeline implementation
- ✅ Include path with dot notation (e.g., `!$ config.database_host`) 
- ✅ File import parsing with proper YAML extension detection
- ❌ Dynamic key support with brackets (e.g., `!$ config[environment]`)
- ❌ Full AST resolution integration (still has some bridging)

**Handlebars Integration:**
- ❌ Complete handlebars helper library
- ❌ String manipulation helpers (currently stubs)
- ❌ Data conversion helpers (toJson, toYaml, base64)

## Revised Implementation Plan

### Phase 1.2: Complete Core Infrastructure ✅ (Already Done)
- [x] YAML AST types and custom tag parsing
- [x] Tag resolution framework
- [x] Environment and variable management

### Phase 1.3: Import System Implementation ✅ (Complete)
- [x] Implement file loader with YAML/JSON parsing
- [x] Add environment variable loader
- [x] Add git information loader (branch, sha, describe)
- [x] Add HTTP/HTTPS loader with async support
- [x] Add random value generator (names, integers)
- [x] Add filehash computation (hex and base64)
- [x] Wire up two-phase processing pipeline
- [x] Implement Phase 1 import loading with handlebars interpolation
- [x] Implement `$imports` and `$defs` key detection and processing
- [x] Add basic tests for two-phase processing functionality
- [x] Fix handlebars string interpolation in Phase 2
- [x] Add dot notation support for include path resolution
- [x] Ensure proper YAML file extension detection for parsing

### Phase 1.4: Advanced Tags Implementation ✅ (Complete)
- [x] Implement missing transformation tags (concatMap, mergeMap, mapListToHash, mapValues, groupBy, fromPairs)
- [x] Add string processing tags (parseYaml, parseJson, escape, toYamlString, toJsonString)
- [x] Add comprehensive tests for all advanced tags
- [x] Fix number representation to preserve integers for CloudFormation compatibility

### Phase 1.5: Include System Enhancement ✅ (Complete)
- [x] Add dot notation support for nested access (already implemented)
- [x] Implement dynamic key support with brackets (e.g., `!$ config[environment]`)
- [x] Add query/selector support for partial inclusion (e.g., `!$ config?database,cache`)
- [x] Add comprehensive tests for enhanced include functionality
- [ ] Wire up include path resolution with imports (deferred to Phase 1.6)

### Phase 1.6: Full AST Resolution
- [ ] Remove temporary AST-to-Value bridge in preprocess.rs
- [ ] Implement full AST resolution pipeline
- [ ] Add comprehensive error handling with stack frames
- [ ] Performance optimization for recursive resolution
- [ ] Refactor resolve_ functions into a trait for different implementations (std vs debug vs trace)

### Phase 1.7: Integration and Testing
- [ ] Complete handlebars helper library
- [ ] Add comprehensive test coverage matching iidy-js behavior
- [ ] Performance benchmarking and optimization
- [ ] Documentation and examples

## Success Criteria

1. **Functional Parity**: Core preprocessing tags work equivalently to iidy-js
2. **Import Compatibility**: Support all major import types from TypeScript version
3. **Type Safety**: All preprocessing operations are type-safe at compile time
4. **Performance**: Preprocessing performance is comparable or better than iidy-js
5. **Integration**: Seamlessly integrates with existing CloudFormation operations
6. **Testing**: Comprehensive offline test coverage with deterministic behavior
7. **Error Handling**: Clear error messages with stack frame context like TypeScript version

## Architecture Notes

### Tag Resolution Refactoring
Currently all tag resolution functions (`resolve_include_tag`, `resolve_if_tag`, etc.) are standalone functions. These should be refactored into a trait-based system to allow for different implementations:

```rust
trait TagResolver {
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value>;
    fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    // ... other tag types
}

struct StandardTagResolver;   // Standard implementation
struct DebugTagResolver;      // With debug logging
struct TracingTagResolver;    // With detailed tracing/metrics
```

This would enable:
- Better testing with mock resolvers
- Debug/tracing modes for troubleshooting
- Performance variants optimized for different use cases
- Easier extensibility for custom tag behavior

## Implementation Progress

### ✅ Phase 1.5 Completion (2025-06-05)

Successfully completed enhanced include system with:

**Dynamic Key Support with Brackets:**
- Variable references: `config[environment]` where `environment` is a variable
- String literals: `config["literal-key"]` and `config['literal-key']`
- Nested paths: `config[env.stage]` where `env.stage` resolves to a variable
- Mixed notation: `configs[environment].regions[region]`

**Query/Selector Support:**
- Single property: `config?database` - select single property
- Multiple properties: `config?database,cache` - select multiple properties
- Nested paths: `config?.database.host` - query with path traversal
- Explicit syntax: `!$ {path: config, query: "database,cache"}`

**Comprehensive Test Coverage:**
- 7 new tests covering all bracket notation scenarios
- 4 new tests covering all query selector scenarios
- All tests passing with full functionality validation

## Next Steps

1. Begin Phase 1.6: Full AST Resolution
2. Remove temporary AST-to-Value bridge in preprocess.rs
3. Implement full AST resolution pipeline
4. Consider tag resolver trait refactoring for better testing and extensibility

---

*Last updated: 2025-06-05*
*Status: Phase 1.5 Complete → Ready for Phase 1.6*