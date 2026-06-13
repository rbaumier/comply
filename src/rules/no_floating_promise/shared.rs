//! Shared data for `no-floating-promise` — the single source of truth for the
//! async-looking method-name list, consumed by both backends.

/// `delete` is intentionally omitted: `Map.prototype.delete`,
/// `Set.prototype.delete`, `WeakMap.prototype.delete`, `WeakSet.prototype.delete`
/// all return `boolean`, and no idiomatic JS/TS API exposes a Promise-returning
/// `.delete(...)` method. Flagging `cache.delete(key)` produces noisy false
/// positives.
pub(super) const ASYNC_LOOKING_METHODS: &[&str] = &[
    "send", "save", "load", "fetch", "query", "emit", "publish", "write", "insert", "update",
    "close", "connect", "dispatch", "sync", "flush", "commit", "rollback", "run", "exec",
    "execute", "process", "handle",
];
