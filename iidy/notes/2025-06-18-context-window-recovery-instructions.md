# Context Window Recovery Instructions

**Date:** 2025-06-18  
**Purpose:** Critical instructions for future Claude instances with empty context windows

## 🚨 IMMEDIATE SITUATION AWARENESS

**You are working on a Rust rewrite of iidy (CloudFormation deployment tool).**

**CRITICAL ISSUE DISCOVERED:** The stack arguments loading system is fundamentally broken and missing ~80% of required functionality compared to the iidy-js reference implementation.

**This is not a minor bug - this is a foundational system failure that makes the Rust implementation unsuitable for production use.**

## 📋 Required Reading Order (MANDATORY)

1. **`/notes/2025-06-18-critical-stack-args-implementation-plan.md`** - Complete analysis and implementation plan
2. **`/notes/2025-06-18-stack-args-loading-analysis.md`** - Detailed requirements analysis  
3. **`@iidy-js-for-reference/src/cfn/loadStackArgs.ts`** - Reference implementation (READ CAREFULLY)
4. **Current todo list** - Use `TodoRead` to see implementation status
5. **`src/stack_args.rs`** - Current broken implementation
6. **`src/stack_args_new.rs`** - Partial new implementation (if exists)

## 🎯 Critical Understanding

### The Problem
- ALL command handlers pass `None` for environment instead of actual environment
- Missing AWS credential configuration system
- Missing $envValues injection (templates depend on this)
- Missing global configuration via SSM parameter store
- Missing CommandsBefore processing
- Missing multi-pass YAML preprocessing

### The Impact
- Environment-based configurations completely broken
- $imports cannot make AWS API calls (no credential setup)
- Production templates fail to load
- Advanced features like CommandsBefore ignored
- Global organizational settings ignored

### The Solution Required
Complete rewrite of stack args loading to match iidy-js functionality with:
- LoadStackArgsContext structure
- AWS credential configuration pipeline  
- Multi-stage processing with proper sequencing
- Integration with all command handlers

## 📊 Current Project Status

Use these commands to assess current state:

```bash
# Check compilation status
cargo check --lib --tests --bins --benches

# Run all tests
cargo nextest r --color=never --hide-progress-bar

# Check todo status  
# Use TodoRead tool

# Check git status
git status
```

## 🚀 Immediate Actions Required

### Priority 1 - Emergency Fixes (Production Blocking)
1. Fix environment parameter in ALL command handlers
2. Implement basic AWS credential configuration  
3. Add $envValues injection
4. Add client request token handling

### Priority 2 - Feature Parity  
1. Implement global configuration via SSM
2. Add multi-pass preprocessing
3. Implement CommandsBefore processing

## 🧭 Navigation

### Key Files
- `src/stack_args.rs` - Current implementation (broken)
- `src/cfn/*.rs` - Command handlers (all need fixes)
- `src/cli.rs` - CLI structure and AWS options
- `src/aws.rs` - AWS configuration (needs extension)

### Reference Files  
- `@iidy-js-for-reference/src/cfn/loadStackArgs.ts` - THE authoritative reference
- `@iidy-js-for-reference/src/configureAWS.ts` - AWS credential config reference

### Documentation
- `notes/2025-06-18-*.md` - Analysis and plans (read these first)
- `CLAUDE.md` - Project instructions and context

## ⚠️ Critical Warnings

### DO NOT:
- Underestimate the complexity - this is a major system rewrite
- Try to take shortcuts - the current implementation is fundamentally flawed  
- Make incremental fixes - the architecture needs complete overhaul
- Ignore the iidy-js reference - it's the authoritative source

### DO:
- Read all the analysis documents first
- Follow the multi-stage processing pipeline exactly
- Test thoroughly with real AWS integration
- Update ALL command handlers consistently

## 🔄 Development Workflow

1. **Assess current state** - Check todos, run tests, review git status
2. **Read documentation** - Review the analysis and plan documents
3. **Study reference** - Understand iidy-js loadStackArgs.ts deeply
4. **Implement systematically** - Follow the phase plan exactly
5. **Test incrementally** - Verify each stage before proceeding
6. **Update todos** - Keep progress tracking current

## 🎯 Success Criteria

### Phase 1 Complete When:
- ✅ All command handlers pass correct environment
- ✅ Basic AWS credential configuration works
- ✅ $envValues injection functional  
- ✅ Client request tokens handled properly
- ✅ Environment-based configs work
- ✅ $imports can make AWS API calls

### Final Success When:
- ✅ Full feature parity with iidy-js loadStackArgs
- ✅ All production workflows functional
- ✅ Comprehensive test coverage
- ✅ Performance equals or exceeds iidy-js

## 🆘 If You're Stuck

1. **Re-read the analysis documents** - The answers are documented
2. **Study the iidy-js code more carefully** - Line-by-line analysis required
3. **Check existing patterns** - How does our YAML preprocessing work?
4. **Test incrementally** - Don't try to implement everything at once
5. **Ask for clarification** - If the requirements are unclear

## 📋 Context Preservation

When your context fills up:

1. **Update todo list** with current progress
2. **Update implementation plan** with discoveries
3. **Commit working code** with detailed commit messages
4. **Document any blockers** in notes/
5. **Create these instructions** for the next instance

Remember: This is the most critical system in iidy. Everything else depends on correct stack args loading. **Do not proceed with other features until this is completely fixed.**