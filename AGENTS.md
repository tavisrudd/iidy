# Development Notes for gptplay

- This environment loses network access after setup, so don't rely on fetching
  crates or git repositories while running commands in subsequent tasks.
- Use the provided `setup.sh` during the setup phase to pre-fetch Rust
  dependencies and clone any external repositories before network access is lost.
- When instructed to run tests, use `cargo test` inside the `iidy` directory.
