# Contributing

## Development workflow

1. Create a branch.
2. Add or update tests with your change.
3. Run local checks:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

4. Open a pull request with:
- Problem statement
- Proposed change
- Test evidence

## Testing guidance

- Prefer targeted unit tests in the module being changed.
- Add integration tests in `tests/cli_integration.rs` for CLI behavior and exit codes.
- Keep deterministic outputs stable (sorting/header rules are contract behavior).

## Error and diagnostics contract

When introducing new validations:

- Emit a stable diagnostic code
- Include actionable `cause` and `action`
- Use the right category for exit-code mapping

## Commit quality bar

Changes should preserve idiomatic Rust style and avoid unnecessary allocations or complexity. Prefer explicit data flow and deterministic behavior over implicit filesystem order.
