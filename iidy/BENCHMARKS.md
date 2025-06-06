# YAML Preprocessing Performance Benchmarks

This document describes the performance benchmarking capabilities for the iidy YAML preprocessing system.

## Benchmark Setup

The project includes comprehensive benchmarks using the Criterion framework for detailed performance analysis, plus a simplified benchmark runner for quick performance validation.

### Available Benchmarks

1. **`yaml_preprocessing_benchmarks.rs`** - Comprehensive Criterion-based benchmarks
2. **`simple_benchmark.rs`** - Lightweight benchmark runner with immediate results

## Running Benchmarks

### Quick Performance Check
```bash
cargo test --bench simple_benchmark test_benchmark_functionality
```

### Full Criterion Benchmarks
```bash
cargo bench --bench yaml_preprocessing_benchmarks
```

### Simplified Benchmarks
```bash
cargo bench --bench simple_benchmark
```

## Benchmark Categories

### 1. Handlebars Template Performance
- **Simple Templates**: Basic variable substitution (`{{name}}-{{environment}}`)
- **Complex Templates**: Multiple helpers (`{{toUpperCase name}}-{{toLowerCase environment}}`)
- **Encoding Templates**: Base64, SHA256 operations

### 2. Tag Resolver Performance
Compares different resolver implementations:
- **StandardTagResolver**: Production implementation (~669ns per operation)
- **DebugTagResolver**: With logging overhead (~11.8µs per operation, 16x slower)
- **TracingTagResolver**: With timing overhead (~4.8µs per operation, 7x slower)

### 3. Complete Preprocessing Pipeline
Tests end-to-end performance with realistic documents:
- **Small Documents**: Basic templates with variables (~466µs)
- **Medium Documents**: With imports, conditionals, transformations (~1.2ms)
- **Large Documents**: Complex CloudFormation templates with nested transformations

### 4. Synchronous vs Asynchronous
Compares sync vs async preprocessing performance to understand overhead.

### 5. Individual Tag Types
Performance characteristics of different preprocessing tags:
- Include tags (`!$`)
- Conditional tags (`!$if`)
- Map transformations (`!$map`)
- Complex nested transformations (`!$concatMap`)

### 6. Memory Scaling
Tests performance with increasing data sizes to identify memory usage patterns and scaling characteristics.

## Performance Results (Debug Build)

Based on initial benchmarking on a development machine:

### Template Processing
- **Simple Handlebars**: ~74µs per template
- **Complex Handlebars**: ~100µs per template (with multiple helpers)

### Tag Resolution
- **Standard Resolver**: ~669ns per tag resolution
- **Debug Resolver**: ~11.8µs per tag (16x overhead for logging)
- **Tracing Resolver**: ~4.8µs per tag (7x overhead for timing)

### Document Processing
- **Small YAML Documents**: ~466µs per document
- **Medium YAML Documents**: ~1.2ms per document (with imports and transformations)

### Performance Insights

1. **Tag Resolver Overhead**: Debug and tracing resolvers add significant overhead (7-16x), making them suitable only for development/debugging.

2. **Template Complexity**: Complex templates with multiple helpers are ~35% slower than simple variable substitution.

3. **Document Size Impact**: Medium documents with imports and transformations are ~2.5x slower than simple templates.

4. **Memory Efficiency**: The preprocessing system handles complex nested transformations efficiently within the microsecond to millisecond range.

## Optimization Opportunities

### Identified Areas for Future Optimization:
1. **Template Caching**: Handlebars template compilation could be cached for repeated use
2. **Import Caching**: File imports could be cached to avoid repeated I/O
3. **Tag Resolution Pooling**: Reuse tag resolver instances to reduce allocation overhead
4. **Async Processing**: Better async/await patterns for I/O-bound operations

## Benchmark Development

### Adding New Benchmarks

To add new benchmarks to the Criterion suite:

```rust
fn bench_new_feature(c: &mut Criterion) {
    let mut group = c.benchmark_group("new_feature");
    
    group.bench_function("operation_name", |b| {
        b.iter(|| {
            // Your benchmark code here
            black_box(operation_to_benchmark())
        })
    });
    
    group.finish();
}

// Add to criterion_group! macro
criterion_group!(benches, bench_new_feature, /* ... existing benchmarks */);
```

### Performance Testing Guidelines

1. **Use `black_box()`** to prevent compiler optimizations from skipping benchmarked code
2. **Include warmup iterations** for stable timing measurements  
3. **Test multiple data sizes** to understand scaling characteristics
4. **Measure both micro-operations and end-to-end workflows**
5. **Use release builds** for production performance measurements

## Continuous Performance Monitoring

The benchmark suite can be integrated into CI/CD pipelines to:
- Track performance regressions over time
- Validate optimization improvements
- Ensure performance requirements are met
- Generate performance reports for releases

## Hardware Considerations

Performance results are highly dependent on:
- CPU architecture and clock speed
- Memory bandwidth and latency
- Storage I/O performance (for import operations)
- Compilation optimizations (debug vs release builds)

Always run benchmarks in release mode (`--release`) for production performance measurements.