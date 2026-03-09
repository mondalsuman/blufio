# ADR-001: ORT for ONNX Inference

**Status:** Accepted
**Date:** 2026-02-XX (decided, Phase 5) / 2026-03-09 (recorded)

## Context and Problem Statement

Blufio needs local ONNX embedding inference for all-MiniLM-L6-v2 (384-dim) in the blufio-memory crate. No external API calls for embeddings -- inference runs in-process. This is an existing decision being formally documented; we chose ORT during Phase 5 (February 2026). Single consumer in a 35-crate workspace.

## Decision Drivers

- Battle-tested ONNX Runtime with full operator coverage
- GPU acceleration path (CUDA, CoreML, DirectML) available when needed
- Pre-built binaries via `download-binaries` feature
- Microsoft backing and active maintenance
- Native ndarray 0.17 interop (workspace-wide constraint)

## Considered Options

1. **ORT** (ort crate) -- Rust bindings to Microsoft ONNX Runtime
2. **Candle** (candle-onnx) -- HuggingFace pure Rust ML framework
3. **tract** (tract-onnx) -- Sonos pure Rust inference engine

### Feature Comparison

| Feature | ORT (rc.11) | Candle | tract |
|---------|------------|--------|-------|
| ONNX operator coverage | Full (ONNX Runtime 1.22) | Partial, undocumented | ~85%, opset 9-18 |
| GPU acceleration | CUDA, CoreML, DirectML, TensorRT | CUDA, Metal | None (CPU-only) |
| Pre-built binaries | Yes (download-binaries feature) | N/A (pure Rust) | N/A (pure Rust) |
| all-MiniLM-L6-v2 | Verified working | Unverified | Likely works |
| Binary size impact | ~40-80MB (bundled .so) | Compile-time only | Compile-time only |
| Docker constraint | Requires glibc (distroless cc) | None (static binary) | None (static binary) |
| Backing | Microsoft | HuggingFace | Sonos |
| Production maturity | High | Low-medium | Medium |
| ndarray interop | Native (0.17) | Own tensor type | Own tensor type |

## Decision Outcome

We chose ORT because it provides battle-tested ONNX Runtime with full operator coverage, verified all-MiniLM-L6-v2 inference, and a GPU acceleration path for future use. The RC pin trade-off is acceptable given single-consumer blast radius.

### Exact Cargo.toml Pin

```toml
ort = { version = "=2.0.0-rc.11", default-features = false, features = ["std", "ndarray", "download-binaries", "copy-dylibs", "tls-rustls"] }
```

Feature flags: `std` (standard library), `ndarray` (tensor interop), `download-binaries` (pre-built ONNX Runtime), `copy-dylibs` (bundle .so into target), `tls-rustls` (HTTPS for binary download).

## Consequences

- **Good:** Verified model inference works, GPU acceleration available when needed, ndarray native interop
- **Bad:** RC pin (`=2.0.0-rc.11`), glibc dependency forces distroless cc Docker image, ~40-80MB binary size from bundled .so files
- **Neutral:** Single consumer (blufio-memory) limits blast radius of RC instability

## Risks and Mitigations

**RC pin risk:** We pin `=2.0.0-rc.11` exact. rc.12 exists (released 2026-03-05) with breaking changes:
- Module restructuring: `ort::tensor` -> `ort::value`
- `IoBinding` and `Adapter` moved into `ort::session`
- Method renames (`with_denormal_as_zero` -> `with_flush_to_zero`)

**ndarray constraint:** ORT rc.11 requires ndarray 0.17, which constrains the workspace.

**Mitigation:** Single consumer in blufio-memory, exact pin prevents accidental upgrade, upgrade checklist below.

## Upgrade Checklist

Trigger: stable 2.0.0 release of the ort crate.

1. Monitor ort crate for stable 2.0.0 release
2. Review breaking changes from rc.11 through stable
3. Update Cargo.toml pin: `=2.0.0-rc.11` to `2.0.0` (remove exact pin)
4. Address module changes: `ort::tensor` -> `ort::value` (rc.12+ change)
5. Run embedding inference tests with all-MiniLM-L6-v2
6. Verify ndarray compatibility (currently 0.17)
7. Test distroless Docker build with new ONNX Runtime .so
8. Update this ADR status to Superseded if API changes are significant

## Related ADRs

- [ADR-002](ADR-002-compiled-in-plugin-architecture.md) -- distroless Docker constraint from ORT .so files affects plugin deployment model

## References

- DOC-01 requirement
- PROJECT.md Key Decisions: "ort 2.0-rc for ONNX inference"
