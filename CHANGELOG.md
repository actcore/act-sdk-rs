# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-04-18

### Changed

- Upgrade to `act:core@0.3.0`. `call-tool` now returns the `tool-result` variant (`immediate(list<tool-event>)` / `streaming(stream<tool-event>)`) instead of `stream<stream-event>`. Non-streaming tools no longer spawn a writer task — the macro returns `Immediate` directly.
- Rename `StreamEvent` → `ToolEvent`, `RawStreamEvent` → `RawToolEvent`, and `IntoResponse::into_stream_events` → `into_tool_events`.
- Vendor the `act:core` WIT directly (unvendored the old `act-wit` git submodule).

### Removed

- `std:streaming` metadata key on tool definitions. The variant of `tool-result` is an implementation detail, not a classification of the tool.

## [0.3.0] - 2026-04-02

### Changed

- **Nested `ComponentInfo` with `std` table** — component metadata is now wrapped in a `std` sub-struct, matching the updated ACT spec
- **CBOR-first response encoding** — non-`IntoResponse` return types are automatically CBOR-encoded; use `Json<T>` to opt into JSON
- Bump wit-bindgen dependency

### Added

- `Json<T>` and `Content` response types for explicit content-type control
- Re-export `Json` and `Content` in prelude
- Use MIME constants (`TEXT_PLAIN`, `APPLICATION_JSON`, etc.) instead of string literals

### Fixed

- Route `serde_json::Value` returns through CBOR encoder correctly

### Removed

- `Value` `IntoResponse` impl — use `send_cbor()`/`send_json()` helpers instead
- `std:accept` metadata constant (content negotiation is a harness concern)

## [0.2.8] - 2026-03-31

### Fixed

- Components no longer need `serde` or `schemars` as direct dependencies — the macro injects `use ::act_sdk::__private::{serde, schemars}` and `#[serde(crate = "...")]` on generated arg structs

## [0.2.7] - 2026-03-31

### Added

- `act.toml` manifest file support — component metadata and capabilities are now read from `act.toml` at compile time, with fallback to `Cargo.toml` and `#[act_component]` attribute overrides
- `FilesystemCap`, `HttpCap`, `SocketsCap` typed capability structs
- `Capabilities::has()`, `Capabilities::fs_mount_root()` helper methods
- Serde `alias` attributes on `ComponentInfo` for dual CBOR/TOML deserialization

### Changed

- **Breaking:** `ComponentInfo.capabilities` is now a typed `Capabilities` struct (was `Vec<ComponentCapability>`)
- `std:capabilities` serializes as a CBOR map keyed by capability ID per spec v0.2.0 (was array of structs)
- `mount-root` moved from top-level `std:fs:mount-root` into `capabilities.wasi:filesystem.mount-root`

### Removed

- `ComponentCapability` struct
- `COMPONENT_FS_MOUNT_ROOT` constant

## [0.2.6] - 2026-03-30

### Added

- `embed_skill!("skill/")` macro — embeds an Agent Skills directory as an `act:skill` WASM custom section (uncompressed tar). See `ACT-AGENTSKILLS.md`.
- SECURITY.md with supply chain and sandbox policies

### Changed

- `#[act_component]` attributes are now optional — `name`, `version`, `description` default to `Cargo.toml` values (`CARGO_PKG_NAME`, `CARGO_PKG_VERSION`, `CARGO_PKG_DESCRIPTION`)

## [0.2.5] - 2026-03-26

### Changed

- Publish workflow now uses crates.io trusted publishing (OIDC) instead of long-lived API token

## [0.2.4] - 2026-03-23

### Fixed

- `decode_content_data` now treats `application/json` as UTF-8 text (same as `text/*`), instead of attempting CBOR decode and falling back to base64
- `IntoResponse for serde_json::Value` now encodes as JSON bytes (`serde_json::to_vec`), not CBOR — previously the data was CBOR-encoded but labeled `application/json`

## [0.2.3] - 2026-03-18

### Added

- `COMPONENT_FS_MOUNT_ROOT` (`std:fs:mount-root`) constant for filesystem mount point metadata
- Capability identifier constants: `CAP_FILESYSTEM`, `CAP_SOCKETS`, `CAP_HTTP`
- `Metadata::extend()` method for merging metadata maps
- `readme` field in all crate manifests for crates.io display

## [0.2.2] - 2026-03-15

### Fixed

- Reverted channel-based streaming to simple Vec buffer — channels don't work in wasm component single-threaded async runtime
- Removed `futures`, `async-channel`, `futures-lite` dependencies from act-sdk

## [0.2.1] - 2026-03-15

### Changed

- Unified `ComponentInfo` type across SDK and host — single `Serialize + Deserialize` struct with `#[non_exhaustive]`, capabilities, and flattened extra metadata
- Complete well-known constants registry (`constants.rs`) — all 34 `std:` keys from the spec
- HTTP types use `serde_with::skip_serializing_none` instead of per-field attributes

### Removed

- `ServerInfo` from HTTP types — replaced by `ComponentInfo`
- `Args` type and `cbor_wrapper!` macro (unused)
- Borrowed `From<&serde_json::Value>` for `Metadata` — only consuming conversion remains

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

[0.3.0]: https://github.com/actcore/act-sdk-rs/compare/0.2.8..0.3.0
[0.2.2]: https://github.com/actcore/act-sdk-rs/compare/0.2.1..0.2.2
[0.2.1]: https://github.com/actcore/act-sdk-rs/compare/0.2.0..0.2.1
[0.2.0]: https://github.com/actcore/act-sdk-rs/compare/0.1.0..0.2.0
[0.1.0]: https://github.com/actcore/act-sdk-rs/tree/0.1.0
