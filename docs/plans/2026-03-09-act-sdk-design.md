# ACT Rust Guest SDK Design

**Date:** 2026-03-09
**Status:** Approved
**Crate:** `act-sdk` in `act-component-sdk-rust/`

## Goal

Provide a Rust SDK for component authors to build ACT components with minimal boilerplate. Replace manual `wit_bindgen::generate!()` + ciborium + serde patterns with proc macros and typed helpers.

## Crate Structure

```
act-component-sdk-rust/
├── Cargo.toml              # Workspace with two crates
├── act-sdk/                # Main crate (re-exports everything)
│   ├── Cargo.toml
│   └── src/lib.rs          # prelude, helpers, types, re-exports macro crate
├── act-sdk-macros/         # Proc macro crate
│   ├── Cargo.toml
│   └── src/lib.rs          # #[act_component], #[act_tool]
└── wit/                    # Vendored act-core.wit (pinned version)
```

Two crates because proc macros must be in a separate crate. `act-sdk` is the only dependency component authors add.

## User-Facing API

### Component Declaration

```rust
use act_sdk::prelude::*;

#[act_component(
    name = "weather-tools",
    version = "0.1.0",
    description = "Weather forecast tools",
    default_language = "en",
)]
struct WeatherComponent;
```

### Tool Signatures — Two Styles

**Style 1: Individual parameters** (simple tools)

```rust
#[act_tool(description = "Greet someone", read_only = true)]
fn greet(#[doc = "Person's name"] name: String) -> ActResult<String> {
    Ok(format!("Hello, {name}!"))
}
```

The macro generates a hidden `#[derive(Deserialize, JsonSchema)]` struct from the params. `#[doc]` attributes become JSON Schema descriptions.

**Style 2: Struct parameters** (complex tools, full schema control)

```rust
#[act_tool(description = "Get weather", read_only = true)]
async fn weather(args: WeatherArgs, ctx: ActContext<WeatherConfig>) -> ActResult<String> {
    Ok(format!("22°C in {}", args.city))
}

#[derive(Deserialize, JsonSchema)]
struct WeatherArgs {
    /// City name
    city: String,
    /// Temperature units
    #[serde(default)]
    units: Option<String>,
}
```

The macro detects style by checking if the first non-context param is a single struct type (style 2) or primitive/multiple params (style 1).

### Streaming Tools

```rust
#[act_tool(description = "Count to n", streaming = true)]
async fn count(n: u32, ctx: ActContext<()>) -> ActResult<()> {
    for i in 1..=n {
        ctx.send_text(format!("{i}")).await?;
        ctx.set_progress(i as u64, n as u64);
    }
    Ok(())
}
```

### Config

```rust
#[derive(Deserialize, JsonSchema)]
struct WeatherConfig {
    api_key: String,
    #[serde(default = "default_base_url")]
    base_url: String,
}
```

Config type is specified via `ActContext<WeatherConfig>`. If any tool uses a config type, the SDK generates `get_config_schema()` from it via `schemars`.

### Possible Function Signatures

```rust
fn tool() -> ActResult<T>                                        // no args, no config, no stream
fn tool(arg1: T1, arg2: T2) -> ActResult<T>                     // individual args
fn tool(args: Args) -> ActResult<T>                              // struct args
fn tool(args: Args, ctx: ActContext<Config>) -> ActResult<T>     // args + config
async fn tool(args: Args, ctx: ActContext<Config>) -> ActResult<()>  // args + config + stream
async fn tool(ctx: ActContext<Config>) -> ActResult<()>          // config + stream, no args
```

## Core SDK Types

```rust
pub type ActResult<T> = Result<T, ActError>;

pub struct ActError {
    pub kind: String,      // e.g. "std:invalid-arguments"
    pub message: String,
}

pub struct ActContext<C = ()> {
    config: C,
    stream: ActStream,
}

impl<C> ActContext<C> {
    pub fn config(&self) -> &C;
    pub async fn send_text(&mut self, text: impl Into<String>) -> ActResult<()>;
    pub async fn send_content(&mut self, part: ContentPart) -> ActResult<()>;
    pub fn set_progress(&mut self, current: u64, total: u64);
}

/// Trait for flexible return types
pub trait IntoResponse {
    fn into_stream_events(self) -> Vec<StreamEvent>;
}

impl IntoResponse for String { /* single text content event */ }
impl IntoResponse for Vec<ContentPart> { /* multiple content events */ }
impl IntoResponse for () { /* empty — for streaming tools */ }
```

`ActStream` is not exposed directly — it's wrapped inside `ActContext`. Streaming is done via `ctx.send_text()` etc.

## What the Macros Generate

### `#[act_component]`

Expands to:
- `wit_bindgen::generate!()` targeting `act-world`
- `Guest` trait implementation that delegates to registered tools
- `get_info()` → `ComponentInfo` from macro attributes
- `get_config_schema()` → JSON Schema from config struct (if any tool uses one)
- `list_tools()` → collects all `#[act_tool]` definitions with JSON Schema from args
- `call_tool()` → dispatch by tool name

The macro owns the WIT generation but does not prevent the user from calling `wit_bindgen::generate!()` for other WIT interfaces in the same crate.

### `#[act_tool]`

Registers the function as a tool. Macro inspects signature to determine:
- Args style (individual params → generate hidden struct, or explicit struct)
- Config presence (detected by `ActContext<T>` param)
- Streaming (detected by `ActContext` presence + async fn + `ActResult<()>` return)
- Return type dispatched via `IntoResponse` trait
- Annotations: `read_only`, `idempotent`, `destructive`, `streaming`, `timeout_ms` → `std:` metadata keys

### Deadlock Prevention

The generated `call_tool` dispatch always:
1. Creates `wit_stream::new::<StreamEvent>()`
2. Spawns the tool function with the writer via `wit_bindgen::spawn()`
3. Returns the `StreamReader<StreamEvent>` immediately

The component author cannot deadlock — the spawn-and-return pattern is enforced by codegen.

## Dependencies

Component authors add only:
```toml
[dependencies]
act-sdk = "0.1"
serde = { version = "1", features = ["derive"] }
schemars = "0.8"
```

`act-sdk` transitively brings:
- `wit-bindgen = "0.53"` (with `async-spawn` feature)
- `ciborium = "0.2"` (CBOR codec)
- `act-sdk-macros` (proc macros)

## Non-Goals

- Host SDK (separate project)
- Runtime/transport concerns (handled by act-host)
- Non-Rust language SDKs
