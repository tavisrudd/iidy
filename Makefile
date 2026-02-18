# iidy Development Makefile

.PHONY: test test-force coverage coverage-html coverage-report clean help test-if-changed fmt clippy lint check-fast

# Test markers directory
TEST_MARKERS_DIR := .make-markers
TEST_LAST_RUN_FILE := $(TEST_MARKERS_DIR)/test-last-run
TEST_NEXTEST_LAST_RUN_FILE := $(TEST_MARKERS_DIR)/nextest-last-run

# Find all relevant source files
RUST_FILES := $(shell find src -name "*.rs" 2>/dev/null)
TOML_FILES := $(shell find . -maxdepth 1 -name "*.toml" 2>/dev/null)
TEST_FILES := $(shell find tests -name "*.rs" 2>/dev/null)
ALL_TRACKED_FILES := $(RUST_FILES) $(TOML_FILES) $(TEST_FILES)

# Default target
help:
	@echo "Available targets:"
	@echo "  check-fast     - Fast check via rust-analyzer (no cargo rebuild)"
	@echo "  check          - Run cargo check for lib, bins, tests, and benches"
	@echo "  test           - Run tests only if source files changed"
	@echo "  test-force     - Force run all tests regardless of changes"
	@echo "  test-nextest   - Run tests with nextest only if source files changed"
	@echo "  coverage       - Generate test coverage summary"
	@echo "  coverage-html  - Generate HTML coverage report"
	@echo "  coverage-report- Open HTML coverage report in browser"
	@echo "  clean          - Clean build artifacts and coverage files"
	@echo "  fmt            - Run cargo fmt"
	@echo "  clippy         - Run clippy lints"
	@echo "  lint           - Run fmt check + clippy"
	@echo "  help           - Show this help message"

# Create markers directory if it doesn't exist
$(TEST_MARKERS_DIR):
	@mkdir -p $(TEST_MARKERS_DIR)

# Fast check via rust-analyzer through ra-multiplex (no cargo rebuild)
check-fast:
	@python3 scripts/ra-check.py

# Run cargo check for full sanity check (rebuilds deps on profile switch)
check:
	cargo check --all-targets

# Format code
fmt:
	cargo fmt

# Run clippy lints
clippy:
	cargo clippy --all-targets -- -D warnings

# Combined lint check (CI-friendly: checks formatting without modifying)
lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

# Run tests only if source files have changed
test: test-if-changed

# Conditional test target based on file changes
test-if-changed: $(TEST_MARKERS_DIR)
	@if [ ! -f "$(TEST_LAST_RUN_FILE)" ] || [ -n "$$(find $(ALL_TRACKED_FILES) -newer $(TEST_LAST_RUN_FILE) 2>/dev/null)" ]; then \
		echo "🔄 Source files changed, running tests..."; \
		if [ "$$CLAUDECODE" = "1" ]; then \
			cargo nextest r --color=never --hide-progress-bar && touch $(TEST_LAST_RUN_FILE); \
		else \
			cargo nextest r && touch $(TEST_LAST_RUN_FILE); \
		fi \
	else \
		echo "OK: no changes since last successful test run"; \
	fi

# Force run all tests regardless of changes
test-force: $(TEST_MARKERS_DIR)
	@echo "🔄 Force running all tests..."
	@if [ "$$CLAUDECODE" = "1" ]; then \
		cargo nextest r --color=never --hide-progress-bar && touch $(TEST_LAST_RUN_FILE); \
	else \
		cargo nextest r && touch $(TEST_LAST_RUN_FILE); \
	fi

# Run nextest only if source files have changed (alias for test-if-changed)
test-nextest: test-if-changed

# cargo build debug
build:
	cargo build

# cargo build --release
release:
	cargo build --release

# Generate coverage summary (fast)
coverage:
	@echo "Generating test coverage summary..."
	cargo tarpaulin --lib --tests --skip-clean --engine llvm --out Stdout --timeout 180 --exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*"

# Generate HTML coverage report
coverage-html:
	@echo "Generating HTML coverage report..."
	cargo tarpaulin --lib --tests --skip-clean --engine llvm --out Html --timeout 300 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--target-dir target/tarpaulin
	@echo "HTML report generated at: tarpaulin-report.html"

# Generate detailed coverage with multiple formats
coverage-report:
	@echo "Generating comprehensive coverage report..."
	cargo tarpaulin --lib --tests --skip-clean --engine llvm --out Html,Json,Lcov --timeout 300 \
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
	@if [ -d "target/tarpaulin" ]; then \
		echo "Removing target/tarpaulin..."; \
		rm -rf target/tarpaulin; \
	fi
	@if [ -d ".make-markers" ]; then \
		echo "Removing .make-markers..."; \
		rm -f .make-markers/*; \
		rmdir .make-markers; \
	fi

# Quick coverage check (for CI/development)
coverage-quick:
	cargo tarpaulin --skip-clean --engine llvm --out Stdout --timeout 120 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--tests --lib

# Coverage with failure threshold
coverage-ci:
	@echo "Running coverage with 70% threshold..."
	cargo tarpaulin --skip-clean --engine llvm --out Stdout --timeout 180 \
		--exclude-files "src/main.rs" "src/demo.rs" "benches/*" "tests/fixtures/*" \
		--fail-under 70 --tests --lib
