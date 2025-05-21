#!/bin/bash
# Setup script for the development environment
# This script installs project dependencies during the setup phase
# when network access is still available.

set -euo pipefail

cd "$(dirname "$0")/iidy"

# Pre-fetch Rust dependencies
cargo fetch

# Optionally build the project to cache compiled dependencies
# cargo build --release

