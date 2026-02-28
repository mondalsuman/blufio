# Contributing to Blufio

## Build

```bash
cargo build --workspace
```

## Test

```bash
cargo test --workspace
```

## Lint

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## License Check

```bash
cargo deny check
```

## Pull Request Requirements

- All tests must pass (`cargo test --workspace`)
- Clippy must be clean (`cargo clippy --workspace --all-targets -- -D warnings`)
- Code must be formatted (`cargo fmt --all --check`)
- License audit must pass (`cargo deny check`)
- New `.rs` files must include the SPDX dual-license header:
  ```rust
  // SPDX-FileCopyrightText: 2026 Blufio Contributors
  // SPDX-License-Identifier: MIT OR Apache-2.0
  ```

## Style

Follow `rustfmt` defaults. No custom formatting rules.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(scope): add new feature
fix(scope): fix bug description
test(scope): add tests for feature
refactor(scope): restructure module
chore(scope): update dependency
docs(scope): update documentation
```

## License

By contributing, you agree that your contributions will be licensed under MIT OR Apache-2.0.
