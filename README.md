# gptplay

This repository contains a small Rust command-line project called `iidy`.

## Pre-installed dependencies

The development environment does not have network access after the container starts. To ensure the Rust dependencies are available, run the `setup.sh` script during the setup phase (when network access is still allowed). This script fetches the project dependencies with `cargo fetch`.

```bash
./setup.sh
```

After running the setup script, you can build or test the project offline with:

```bash
cd iidy
cargo build
```

