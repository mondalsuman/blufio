# Blufio

An always-on personal AI agent.

[![CI](https://github.com/blufio/blufio/actions/workflows/ci.yml/badge.svg)](https://github.com/blufio/blufio/actions/workflows/ci.yml)

## Build from Source

```bash
# Clone
git clone https://github.com/blufio/blufio.git
cd blufio

# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Run
cargo run -- --help
```

## Architecture

Blufio is organized as a Cargo workspace:

| Crate | Purpose |
|-------|---------|
| `blufio` | CLI binary with jemalloc allocator |
| `blufio-core` | Core traits, error types, and shared types |
| `blufio-config` | TOML configuration with validation and diagnostics |

## License

Licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
