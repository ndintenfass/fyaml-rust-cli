# FYAML

FYAML (filesystem-backed YAML) is a one-way convention for packing a directory tree of YAML fragments into one deterministic document.

`fyaml` is a Rust CLI that implements the FYAML spec in `fyaml-spec.md`.

## One-way contract

FYAML packing is intentionally one-way: many directory layouts can pack to the same YAML output. The original layout is not recoverable from the packed YAML.

## Features

- Deterministic `pack` with canonical key ordering
- `validate` with human and machine-readable diagnostics
- `explain` trace for derived keys, ignored files, and directory mode decisions
- Semantic `diff` between two FYAML trees
- Deterministic `scaffold` helper (explicitly non-invertible)

## Quick start

Prerequisites:

- Rust toolchain (stable)
- `cargo`

Build:

```bash
cargo build
```

Run:

```bash
cargo run -- pack ./example
cargo run -- validate ./example
cargo run -- explain ./example
```

Run tests:

```bash
cargo test
```

Lint and format:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Command reference

```bash
fyaml pack <DIR> [-o <FILE>] [--format yaml|json] [flags...]
fyaml validate <DIR> [--json] [--strict] [flags...]
fyaml explain <DIR> [--json] [flags...]
fyaml diff <DIR_A> <DIR_B> [--format path|json] [flags...]
fyaml scaffold <INPUT.yml> <DIR> [--layout flat|nested|hybrid] [--seq dir|files]
```

See `fyaml --help` for full flag docs.

## Design notes

- Hidden entries are ignored by default (`--include-hidden` to include).
- Sequence directories are detected when all contributing keys are numeric.
- Mixed numeric and non-numeric contributors are hard errors.
- Dotted filename keys warn by default (`--allow-dotted-keys` suppresses warning).
- Reserved YAML keys (`true`, `false`, `yes`, `no`, `null`, `on`, `off`) are errors by default (`--allow-reserved-keys` to allow).

## CI

This repository includes both:

- GitHub Actions workflow at `.github/workflows/ci.yml`
- CircleCI pipeline at `.circleci/config.yml`

Both run format checks, clippy, and tests.

## Contributing

See `CONTRIBUTING.md`.

## License

MIT (`LICENSE`).
