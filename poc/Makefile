# RGB Sandbox Makefile
# Provides convenient commands for development and testing

.PHONY: help test coverage build clean install lint fmt check docs

# Default target
help:
	@echo "RGB Sandbox Development Commands"
	@echo "================================"
	@echo ""
	@echo "Testing:"
	@echo "  test          Run all tests"
	@echo "  test-unit     Run unit tests only"
	@echo "  test-integ    Run integration tests only"
	@echo "  coverage      Generate coverage report"
	@echo "  cov-html      Generate HTML coverage report"
	@echo "  cov-open      Generate and open HTML coverage report"
	@echo ""
	@echo "Code Quality:"
	@echo "  lint          Run clippy lints"
	@echo "  fmt           Format code"
	@echo "  check         Check code without building"
	@echo "  clean         Clean build artifacts"
	@echo ""
	@echo "Build:"
	@echo "  build         Build the project"
	@echo "  install       Install cargo-llvm-cov tools"
	@echo ""
	@echo "Documentation:"
	@echo "  docs          Generate documentation"
	@echo "  docs-open     Generate and open documentation"

# Testing targets
test:
	cargo test --all-features --workspace

test-unit:
	cargo test --lib --all-features

test-integ:
	cargo test --test '*' --all-features

# Coverage targets
coverage:
	@echo "🔍 Generating comprehensive coverage report..."
	@./scripts/coverage.sh

cov-html:
	@echo "📊 Generating HTML coverage report..."
	@mkdir -p coverage
	cargo llvm-cov --all-features --html --output-dir coverage/html
	@echo "✅ HTML report generated: coverage/html/index.html"

cov-open: cov-html
	@if command -v open >/dev/null 2>&1; then \
		open coverage/html/index.html; \
	elif command -v xdg-open >/dev/null 2>&1; then \
		xdg-open coverage/html/index.html; \
	else \
		echo "Please open coverage/html/index.html in your browser"; \
	fi

# Code quality targets
lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

# Build targets
build:
	cargo build --all-features

build-release:
	cargo build --release --all-features

# Installation target
install:
	@echo "🔧 Installing required tools..."
	rustup component add llvm-tools-preview
	cargo install cargo-llvm-cov

# Maintenance targets
clean:
	cargo clean
	rm -rf coverage/
	rm -rf coverage-reports/
	rm -rf target/

# Documentation targets
docs:
	cargo doc --all-features --workspace --no-deps

docs-open:
	cargo doc --all-features --workspace --no-deps --open

# Development workflow
dev-setup: install
	@echo "🚀 Development environment setup complete!"
	@echo "Run 'make test' to verify everything works."

dev-check: fmt-check lint test coverage
	@echo "✅ All development checks passed!"

# CI simulation
ci: dev-check
	@echo "🤖 CI simulation complete!"

# Quick development cycle
quick: fmt lint test
	@echo "⚡ Quick development check complete!"