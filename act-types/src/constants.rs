//! Well-known constants used across the ACT protocol.
//!
//! Authoritative list: `ACT-CONSTANTS.md` in the spec repo.

// ── Error kinds ──

pub const ERR_NOT_FOUND: &str = "std:not-found";
pub const ERR_INVALID_ARGS: &str = "std:invalid-args";
pub const ERR_TIMEOUT: &str = "std:timeout";
pub const ERR_CAPABILITY_DENIED: &str = "std:capability-denied";
pub const ERR_INTERNAL: &str = "std:internal";

// ── Component info keys ──

pub const COMPONENT_NAME: &str = "std:name";
pub const COMPONENT_VERSION: &str = "std:version";
pub const COMPONENT_DESCRIPTION: &str = "std:description";
pub const COMPONENT_DEFAULT_LANGUAGE: &str = "std:default-language";
pub const COMPONENT_CAPABILITIES: &str = "std:capabilities";
pub const COMPONENT_SKILL: &str = "std:skill";
pub const COMPONENT_FS_MOUNT_ROOT: &str = "std:fs:mount-root";

// ── Capability identifiers ──

pub const CAP_FILESYSTEM: &str = "wasi:filesystem";
pub const CAP_SOCKETS: &str = "wasi:sockets";
pub const CAP_HTTP: &str = "wasi:http";

// ── Tool definition metadata keys ──

pub const META_READ_ONLY: &str = "std:read-only";
pub const META_IDEMPOTENT: &str = "std:idempotent";
pub const META_DESTRUCTIVE: &str = "std:destructive";
pub const META_STREAMING: &str = "std:streaming";
pub const META_TIMEOUT_MS: &str = "std:timeout-ms";
pub const META_USAGE_HINTS: &str = "std:usage-hints";
pub const META_ANTI_USAGE_HINTS: &str = "std:anti-usage-hints";
pub const META_EXAMPLES: &str = "std:examples";
pub const META_TAGS: &str = "std:tags";

// ── Content part metadata keys ──

pub const META_PROGRESS: &str = "std:progress";
pub const META_PROGRESS_TOTAL: &str = "std:progress-total";

// ── Cross-cutting metadata keys ──

pub const META_TRACEPARENT: &str = "std:traceparent";
pub const META_TRACESTATE: &str = "std:tracestate";
pub const META_REQUEST_ID: &str = "std:request-id";
pub const META_PROGRESS_TOKEN: &str = "std:progress-token";

// ── Bridge metadata keys ──

pub const META_FORWARD: &str = "std:forward";

// ── Authentication metadata keys ──

pub const AUTH_API_KEY: &str = "std:api-key";
pub const AUTH_BEARER_TOKEN: &str = "std:bearer-token";
pub const AUTH_USERNAME: &str = "std:username";
pub const AUTH_PASSWORD: &str = "std:password";

// ── Event kinds ──

pub const EVENT_TOOLS_CHANGED: &str = "std:tools:changed";
pub const EVENT_RESOURCES_CHANGED: &str = "std:resources:changed";
pub const EVENT_EVENTS_CHANGED: &str = "std:events:changed";

// ── Resource URIs ──

pub const RESOURCE_ICON: &str = "std:icon";

// ── WASM custom sections ──

pub const SECTION_ACT_COMPONENT: &str = "act:component";

// ── MIME types ──

pub const MIME_JSON: &str = "application/json";
pub const MIME_CBOR: &str = "application/cbor";
pub const MIME_TEXT: &str = "text/plain";
pub const MIME_SSE: &str = "text/event-stream";
