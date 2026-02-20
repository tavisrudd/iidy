# Contributing to iidy

## Setup

After cloning, configure git hooks:

```
make setup
```

This points git at the tracked `.githooks/` directory, which runs `cargo fmt`
and `cargo clippy` before each commit.

## Building and testing

```
make check       # cargo check + clippy (~46s)
make test        # full test suite (~2min)
make build       # release build
```

`make test` tracks source changes and skips if nothing changed.

## Snapshot tests

Template examples in `example-templates/` are automatically snapshot-tested
using [insta](https://insta.rs/). When your changes produce new or changed
snapshots:

1. Run `make test` -- it will fail on the first new/changed snapshot
2. Review the `.snap.new` files in `tests/snapshots/`
3. Accept with `cargo insta accept`
4. Re-run `make test` to confirm all pass

## Coding standards

- Imports at the top of the file, not inside functions
- Meaningful names over comments -- comment only the non-obvious
- All tests must be offline and deterministic (use fixture data, no AWS calls)
- No compiler warnings -- `make check` must be clean before submitting

## Submitting changes

1. Fork the repo and create a branch
2. Make your changes
3. Ensure `make check` and `make test` pass with zero warnings and zero failures
4. Open a pull request with a clear description of what and why

## Reporting issues

Open an issue on GitHub with:
- What you expected to happen
- What actually happened
- The command you ran and its output
- Your OS and Rust version (`rustc --version`)
