# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.13.0] - 2026-06-26

### Changed

- **`act-types`: `wasi:sockets` capability grants may now omit `ports`** to allow
  any port. `SocketsAllow.ports` is now `Option<Vec<u16>>` (`None`/omitted = any
  port); previously a non-empty list was required. Breaking for `act-types`
  consumers that construct or match on `SocketsAllow.ports`. `act-sdk` and
  `act-sdk-macros` are version-only bumps (no API change).

## [0.12.0] - 2026-06-26

This release migrates the SDK to `act:tools@0.2.0` and `act:sessions@0.2.0`
(`act:core@0.4.0` is unchanged).

### Changed

- **Migrated to `act:tools@0.2.0` / `act:sessions@0.2.0` (breaking).** Each
  package extracted its data model into a new function-free, stream-free `types`
  interface. `tool-definition`, `content-part`, `tool-event` and
  `list-tools-response` now live in `act:tools/types`; the `session` record now
  lives in `act:sessions/types`. `tool-result` (the only `stream<>`-bearing
  type) and the async `tool-provider` functions stay in `tool-provider`; the
  `session-provider` functions stay in `session-provider`.
- **Generated-binding paths shifted accordingly.** In the Rust bindings, types
  that `tool-provider` does not `use` directly (`ToolDefinition`, `ContentPart`)
  and the `act:core` `LocalizedString` now resolve under `act::tools::types` /
  `act::core::types` rather than `exports::act::tools::tool_provider`.
  `ToolResult` and the `Guest` traits keep their previous locations. The
  `#[act_component]` macro and the raw-binding examples were updated to match.
- Vendored WIT deps under `wit/deps/` bumped to `act-tools-0.2.0` /
  `act-sessions-0.2.0`; example world files now export the `@0.2.0` interfaces.

## [0.11.0] - 2026-06-22

### Added

- **Typed filesystem mounts.** Components can declare `bind`/`root` mounts under
  `params.mounts`; `Capabilities::fs_mounts()` parses them into typed entries, and
  a new `validate_mounts()` helper checks them at build time.

## [0.10.0] - 2026-06-18

This release replaces the per-class capability types with a single uniform model.

### Changed

- **Uniform capability model (breaking).** The per-class structs
  (`FilesystemCap`/`HttpCap`/`SocketsCap`) are replaced by one
  `CapabilityRequest` + provider-defined `Constraint` envelope, keyed by
  capability id. `Capabilities` now serializes as a map keyed by id.

### Fixed

- `LocalizedString` now serializes untagged (a bare string or a map) instead of
  `{"Plain": "…"}`.

### Removed

- Dropped the unused `CapabilityRequest.optional` field (YAGNI).

## [0.9.0] - 2026-06-16

This release moves component metadata out of the SDK macros into `act-build pack`.
Components run `act-build pack` after `cargo build` (the canonical flow) — a bare
`cargo build` no longer embeds `act:component`. `pack` resolves metadata from the
**language project declaration** (`Cargo.toml` `[package]`, `pyproject.toml`,
`package.json`) merged with `act.toml`.

### Changed

- **`#[act_component]` is lean and takes no arguments.** It compiles only
  component logic (WIT world, `list_tools`/`call_tool` dispatch, session-provider).
  It no longer reads the project manifest or embeds the `act:component` /
  `version` / `description` sections — `act-build pack` is now the sole metadata
  embedder, resolving name/version/description from the language project
  declaration (`Cargo.toml` `[package]`, preferred), merged with `act.toml`, with
  `act-build pack --set std.name=…` for feature-conditional overrides.
- **Tool return encoding unified behind `IntoToolResponse`** (renamed from
  `IntoResponse`), resolved by autoref specialization: `String`/`&str`→`text/plain`,
  `Vec<u8>`→`application/octet-stream`, `Content`→its MIME, `Json<T>`→
  `application/json`, any other `Serialize` value (incl. `Bytes`)→`application/cbor`.
  No behavior change for components.

### Removed

- **`embed_skill!`** — `act-build pack` embeds the `skill/` directory into `act:skill`.
- `#[act_component]`'s `name` / `version` / `description` / `manifest` arguments —
  metadata comes from the language project declaration (`Cargo.toml` `[package]`,
  preferred), `act.toml`, or `act-build pack --set`.
- `SessionRegistry`'s `Default` impl — construct with `SessionRegistry::new("<prefix>")`.

## [0.8.2] - 2026-06-15

### Changed

- **`Bytes` is now envelope-only.** It serializes and deserializes strictly as a
  CBOR byte string — the `{"$bytes":"<base64>"}` envelope on JSON transports —
  and no longer accepts a bare base64 string (a string is text, not bytes). Its
  JSON Schema is now the `$bytes` object, and returning `Bytes` from a tool
  yields the envelope (use `Content("image/png", …)` for raw mime-typed blobs).
  One consistent rule now holds across all binary types: a bare string is always
  text; bytes always travel as an envelope.

## [0.8.1] - 2026-06-15

### Added

- **`Bytes` type for binary tool fields** — serializes to a CBOR byte string
  (major type 2), deserializes from either a byte string or a base64 string, and
  advertises `contentEncoding: base64` in its JSON Schema. Returning `Bytes`
  produces `application/octet-stream`. Exported from the prelude.
- **Lossless binary round-trip across JSON transports** — CBOR byte strings now
  project to and from the canonical `{"$bytes":"<base64>"}` JSON wrapper (with
  `$`-prefixed key escaping), so binary data survives HTTP+JSON and MCP without
  base64-into-text corruption.

## [0.8.0] - 2026-06-13

### Changed

- Adopted the WASI 0.3 (final) toolchain. The SDK now builds against
  `wit-bindgen` 0.58, and the HTTP examples target `wasip3` 0.7.0 (the
  ratified `wasi:0.3.0`). **Breaking:** components built with the SDK must
  bump their own `wit-bindgen` dependency to 0.58.
- The tool-call macro now generates `wit_bindgen::spawn_local` (the function
  was renamed from `spawn` in wit-bindgen 0.58).

## [0.7.1] - 2026-05-24

### Added
- **`include!` support in `#[act_component]`.** `#[act_tool]` functions can now
  live in separate files and be pulled into the component module via
  `include!("path")`, so large components (e.g. 100+ tools) can be split across
  many modules instead of one giant `lib.rs`. Included paths resolve relative to
  `src/`.
- **`wasi:sockets` capability declarations.** `SocketsCap` now carries `allow`
  entries (host/CIDR + required ports + optional protocols, defaulting to
  TCP+UDP), declaring the capability ceiling for raw TCP/UDP I/O. Default
  protocols are omitted on serialization to keep manifest round-trips clean.

## [0.7.0] - 2026-05-06

### Added
- **act:sessions/session-provider macro support.** `wit/deps/act-sessions`
  is bundled in the SDK; consumers symlink it like the other interfaces
  and add `export act:sessions/session-provider@0.1.0;` to their
  `world.wit`.
- **`act_sdk::SessionRegistry<T>`** — interior-mutable id→state map for
  components that maintain per-session state. Allocates ids as
  `<prefix>_<n>`.
- **`#[session_open]` and `#[session_close]` markers** — when both appear
  inside `#[act_component]`, the macro generates the session-provider
  Guest impl. `get-open-session-args-schema` is derived from the open
  fn's args type via `JsonSchema`; `open-session` decodes metadata-shaped
  args; `close-session` is a sync pass-through (per WIT).
- `act_sdk::sessions::session_id_from_metadata` — pulls
  `std:session-id` out of WIT metadata (CBOR-decoded).
- New constants in `act_types::constants`: `META_SESSION_ID`,
  `META_AGENT_ID`, `META_SESSION_OP`, `ERR_SESSION_NOT_FOUND`.
  `ActError::session_not_found` helper.
- New `act_types::http` wire types: `OpenSessionRequest`,
  `OpenSessionResponse`. `error_kind_to_status` now maps
  `std:session-not-found` to **404**.

### Migration
No breaking changes for tool-only components — the macro is opt-in
through the new `#[session_open]`/`#[session_close]` markers and the
extra `wit/deps/act-sessions` symlink.

## [0.6.0] - 2026-04-29

### Changed
- Migrate to the split WIT layout from `act-spec`: `act:core@0.4.0`
  (cross-cutting types only) plus `act:tools@0.1.0` (tool-provider).
- `#[act_component]` emits bindings against
  `exports::act::tools::tool_provider::*` and uses the renamed `Error`
  type (was `ToolError`).
- `Guest::call_tool` takes flat `(name, arguments, metadata)` parameters
  instead of a `ToolCall` record. Tool function bodies authored with
  `#[act_tool]` are unaffected; raw-bindgen components need their
  `Guest` impl updated.

### Removed
- `Guest::get_metadata_schema` — the corresponding WIT function is gone
  in `act:tools@0.1.0`. A schema-discovery mechanism for per-call
  metadata is planned for a future minor version.
- `MetadataSchemaRequest` from `act-types::http` (no `/metadata-schema`
  endpoint).

### Migration
Components on previous SDK versions need to:
1. Bump to this SDK version.
2. Run `wit-deps` (or sync `wit/deps/` manually) to pick up the new
   WIT layout — `act-core` is types-only, `act-tools` is added.
3. Rebuild — the world now exports `act:tools/tool-provider@0.1.0`.

## [0.5.0] - 2026-04-21

### Added

- **`FilesystemCap.allow` + `FilesystemAllow`** — components declare the exact paths they need as glob patterns, each with a required `mode` of `"ro"` or `"rw"`. Exported `FsMode` enum (`Ro` / `Rw`) for use in manifests and downstream tools.
- **`HttpCap.allow` + `HttpAllow`** — components declare HTTP peers by host (exact hostname, `*.suffix` wildcard, or `*` for any). Optional narrowers: `scheme`, `methods`, `ports`. No `cidr`, no `deny` — declarations are positive-only ceilings.

### Changed

- **Tool names now emit in snake_case.** Previously the proc macro replaced underscores with hyphens; now the raw identifier is used as-is. Components relying on the hyphen form must update any hardcoded tool-name expectations.
- **`HttpCap` no longer derives `Copy`.** It now contains a `Vec<HttpAllow>`. Callers that copied the zero-sized struct must clone.
- **Version bumped from 0.4.0 → 0.5.0** to align with act-cli 0.5.0. When the pair is used together, act-cli intersects the SDK-declared `allow` arrays with the user's policy as an enforcing ceiling — undeclared or empty-declared capability classes are hard-denied regardless of user policy.

### Fixed

- Stale reference to the old `StreamEvent` WIT type in `RawToolEvent`'s doc comment (the WIT type has been `ToolEvent` since `act:core@0.3.0`).

## [0.4.0] - 2026-04-18

### Changed

- Upgrade to `act:core@0.3.0`. `call-tool` now returns the `tool-result` variant (`immediate(list<tool-event>)` / `streaming(stream<tool-event>)`) instead of `stream<stream-event>`. Non-streaming tools no longer spawn a writer task — the macro returns `Immediate` directly.
- Rename `StreamEvent` → `ToolEvent`, `RawStreamEvent` → `RawToolEvent`, and `IntoResponse::into_stream_events` → `into_tool_events`.
- Vendor the `act:core` WIT directly (unvendored the old `act-wit` git submodule).
- Bump `wit-bindgen` to 0.57.

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
