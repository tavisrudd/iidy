# iidy Development Makefile

.PHONY: test coverage coverage-html coverage-report clean help

# Default target
help:
	@echo "Available targets:"
	@echo "  test           - Run all tests"
	@echo "  coverage       - Generate test coverage summary"
	@echo "  coverage-html  - Generate HTML coverage report"
	@echo "  coverage-report- Open HTML coverage report in browser"
	@echo "  clean          - Clean build artifacts and coverage files"
	@echo "  help           - Show this help message"

# Run all tests
test:
	cargo test --all-features

# Generate coverage summary (fast)
coverage:
	@echo "Generating test coverage summary..."
	cargo tarpaulin --skip-clean --engine llvm --out Stdout --timeout 180 --exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*"

# Generate HTML coverage report
coverage-html:
	@echo "Generating HTML coverage report..."
	cargo tarpaulin --skip-clean --engine llvm --out Html --timeout 300 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--target-dir target/tarpaulin
	@echo "HTML report generated at: tarpaulin-report.html"

# Generate detailed coverage with multiple formats
coverage-report:
	@echo "Generating comprehensive coverage report..."
	cargo tarpaulin --skip-clean --engine llvm --out Html,Json,Lcov --timeout 300 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--target-dir target/tarpaulin
	@echo "Coverage reports generated:"
	@echo "  - HTML: tarpaulin-report.html"
	@echo "  - JSON: cobertura.json"
	@echo "  - LCOV: lcov.info"

# Open HTML coverage report in browser (macOS)
coverage-open: coverage-html
	@if [ -f "tarpaulin-report.html" ]; then \
		open tarpaulin-report.html; \
	else \
		echo "HTML coverage report not found. Run 'make coverage-html' first."; \
	fi

# Clean build artifacts and coverage files
clean:
	cargo clean
	rm -f tarpaulin-report.html
	rm -f cobertura.json  
	rm -f lcov.info
	rm -rf target/tarpaulin

# Quick coverage check (for CI/development)
coverage-quick:
	@echo "Running quick coverage check..."
	cargo tarpaulin --skip-clean --engine llvm --out Stdout --timeout 120 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--tests --lib

# Coverage with failure threshold
coverage-ci:
	@echo "Running coverage with 70% threshold..."
	cargo tarpaulin --skip-clean --engine llvm --out Stdout --timeout 180 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--fail-under 70 --tests --lib