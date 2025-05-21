# gptplay

This repository contains a small Rust command-line project called `iidy` which is a port of the typescript version at https://github.com/unbounce/iidy.

## Notes to Codex Agent


The Codex development environment does not have network access after the container starts. The Rust dependencies are installed prior to Codex agent execution via the `setup.sh` script during the setup phase (when network access is still allowed). The script fetches the Cargo dependencies and also clones the upstream [`unbounce/iidy`](https://github.com/unbounce/iidy) repository for reference into iidy-js/
