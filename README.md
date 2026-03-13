<p align="center">
  <strong>act-sdk-rs</strong><br>
  <em>Build AI-ready WebAssembly components in Rust</em>
</p>

<p align="center">
  <a href="https://github.com/actcore/act-sdk-rs/actions"><img src="https://github.com/actcore/act-sdk-rs/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/act-sdk"><img src="https://img.shields.io/crates/v/act-sdk.svg" alt="crates.io"></a>
  <a href="https://docs.rs/act-sdk"><img src="https://docs.rs/act-sdk/badge.svg" alt="docs.rs"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
</p>

---

Rust SDK for the [ACT protocol](https://github.com/actcore/act-spec) — define tools as plain functions, compile to `.wasm`, serve over MCP or HTTP.

## Quick start

```rust
use act_sdk::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct GreetArgs {
    name: String,
}

#[act_component(name = "greeter", version = "0.1.0", description = "A greeter")]
mod component {
    use super::*;

    #[act_tool(description = "Say hello", read_only)]
    fn greet(args: GreetArgs) -> ActResult<String> {
        Ok(format!("Hello, {}!", args.name))
    }
}
```

```sh
cargo build --target wasm32-wasip2 --release
act serve target/wasm32-wasip2/release/greeter.wasm
# or
act mcp target/wasm32-wasip2/release/greeter.wasm
```

## Workspace crates

| Crate | Description |
|-------|-------------|
| [`act-sdk`](act-sdk/) | SDK with `#[act_component]` and `#[act_tool]` macros |
| [`act-sdk-macros`](act-sdk-macros/) | Proc macro implementation |
| [`act-types`](act-types/) | Shared types, CBOR utilities, JSON-RPC and MCP wire formats |

## Features

- **`#[act_tool]`** — derive tool metadata, JSON Schema, and CBOR serialization from a plain Rust function
- **Streaming** — mark tools with `streaming` and use `ActContext` to emit progress events
- **Typed config** — define a config struct, get automatic JSON Schema generation and validation
- **MCP + HTTP** — same `.wasm` works with both transports, zero code changes

## Examples

| Example | What it shows |
|---------|---------------|
| [`hello-sdk`](examples/hello-sdk/) | Basic tools, streaming with `ctx.send_progress()` |
| [`config-sdk`](examples/config-sdk/) | Typed component configuration |
| [`http-client-sdk`](examples/http-client-sdk/) | Outbound HTTP via WASI |
| [`hello-world`](examples/hello-world/) | Minimal component without SDK (raw wit-bindgen) |
| [`http-client`](examples/http-client/) | Raw HTTP client without SDK |
| [`counter`](examples/counter/) | Stateful counter |

## Building

Requires Rust nightly (for `wasm32-wasip2` async support):

```sh
# Build all examples
cargo build --target wasm32-wasip2 --release

# Run tests (host-side, not wasm)
cargo test --target x86_64-unknown-linux-gnu
```

See [`rust-toolchain.toml`](rust-toolchain.toml) for the pinned toolchain.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
