#!/bin/bash
# Setup script for the development environment
# This script installs project dependencies during the setup phase
# when network access is still available.

set -euo pipefail

cd "$(dirname "$0")"

# Clone the upstream unbounce/iidy repository for reference
if [ ! -d "unbounce-iidy/.git" ]; then
    git clone --depth 1 https://github.com/unbounce/iidy.git unbounce-iidy
fi

# Pre-fetch Rust dependencies for this project
(cd iidy && cargo fetch)

# Optionally build the project to cache compiled dependencies
# (cd iidy && cargo build --release)

