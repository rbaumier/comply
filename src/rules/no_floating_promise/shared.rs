//! Shared data for `no-floating-promise` — the single source of truth for the
//! async-looking method-name list, consumed by both backends.

/// Method names whose presence is treated as a signal that a discarded
/// statement-level call may return a Promise.
///
/// `close`, `write`, `emit`, and `send` are intentionally excluded: in the
/// Node.js ecosystem they are dominated by synchronous, callback-based APIs that
/// return non-Promise values — `http.Server.close([cb])` returns the `Server`,
/// `stream.write(chunk)` and `EventEmitter.emit(event)` return `boolean`, and
/// `WebSocket.send(data)` returns `void`. A name-only match on these produces
/// more false positives than true positives.
///
/// `delete` is likewise excluded: `Map`/`Set`/`WeakMap`/`WeakSet` `.delete(...)`
/// all return `boolean`, and no idiomatic JS/TS API exposes a Promise-returning
/// `.delete(...)` method.
pub(super) const ASYNC_LOOKING_METHODS: &[&str] = &[
    "save", "load", "fetch", "query", "publish", "insert", "update", "connect", "dispatch",
    "sync", "flush", "commit", "rollback", "run", "exec", "execute", "process", "handle",
];
