//! Well-known constants used across the ACT protocol.

// ── Error kinds ──

pub const ERR_NOT_FOUND: &str = "std:not-found";
pub const ERR_INVALID_ARGS: &str = "std:invalid-args";
pub const ERR_TIMEOUT: &str = "std:timeout";
pub const ERR_CAPABILITY_DENIED: &str = "std:capability-denied";
pub const ERR_INTERNAL: &str = "std:internal";

// ── Metadata keys ──

pub const META_READ_ONLY: &str = "std:read-only";
pub const META_IDEMPOTENT: &str = "std:idempotent";
pub const META_DESTRUCTIVE: &str = "std:destructive";
pub const META_STREAMING: &str = "std:streaming";
pub const META_TIMEOUT_MS: &str = "std:timeout-ms";
pub const META_PROGRESS: &str = "std:progress";
pub const META_PROGRESS_TOTAL: &str = "std:progress-total";

// ── Resource URIs ──

pub const RESOURCE_ICON: &str = "std:icon";

// ── Event kinds ──

pub const EVENT_TOOLS_CHANGED: &str = "std:tools:changed";
pub const EVENT_RESOURCES_CHANGED: &str = "std:resources:changed";
pub const EVENT_EVENTS_CHANGED: &str = "std:events:changed";

// ── MIME types ──

pub const MIME_JSON: &str = "application/json";
pub const MIME_CBOR: &str = "application/cbor";
pub const MIME_TEXT: &str = "text/plain";
pub const MIME_SSE: &str = "text/event-stream";
