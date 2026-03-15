# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-15

Breaking release aligned with ACT spec v0.2.0 — `config` replaced by `metadata` throughout.

### Changed

- **Spec v0.2.0**: `get-info` and `get-config-schema` removed from WIT; `get-metadata-schema(metadata)` added; `call-tool` takes only `tool-call` (metadata inside)
- **Component metadata**: stored in `act:component` WASM custom section (CBOR-encoded) instead of `get-info()` export; standard `version`/`description` sections also generated
- **Streaming architecture**: two-channel design — unbounded (buffered) for `send_text()`/`send_content()`, bounded(0) (backpressure) via `ctx.writer()`
- **`ActContext`**: `ctx.config()` renamed to `ctx.metadata()`; `send_text()`/`send_content()` are now sync (non-blocking)
- **`#[act_tool]`**: struct args require explicit `#[args]` attribute instead of heuristic detection
- **HTTP types**: `ConfigRequest` replaced by `MetadataRequest`/`MetadataSchemaRequest`; protocol version bumped to 0.2

### Added

- `#[args]` attribute for explicit struct-style tool parameters with `#[serde]` support
- `ctx.writer()` for direct backpressure streaming via `async-channel` bounded(0)
- `darling` for declarative proc macro attribute parsing

### Removed

- `Config` type from act-types
- `get-info()` / `get-config-schema()` from tool-provider trait
- `send_progress()` from `ActContext`
- Implicit struct args detection heuristic

## [0.1.0] - 2026-03-14

Initial release of the ACT Rust SDK — a toolkit for building WebAssembly components that implement the `act:core` protocol.

### Added

- `act-sdk` crate with `#[act_component]` and `#[act_tool]` proc macros for declarative component authoring
- `act-sdk-macros` proc macro crate powering the SDK
- `act-types` crate with shared types, CBOR helpers, JSON-RPC and MCP wire-format modules
- Example components: `hello-sdk`, `config-sdk`, `http-client-sdk`
- CI pipeline with clippy, tests, formatting, and wasm32-wasip2 builds
- E2E test infrastructure for components
- Release pipeline with git-cliff changelog generation

[0.2.0]: https://github.com/actcore/act-sdk-rs/compare/0.1.0..0.2.0
[0.1.0]: https://github.com/actcore/act-sdk-rs/tree/0.1.0
