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
///
/// `update` is likewise excluded: it is dominated by synchronous mutation APIs —
/// Angular's `WritableSignal.update(fn)` returns `void`, Immutable.js `.update()`
/// returns the collection, and store/Map-like `.update(...)` are synchronous. The
/// name alone is too weak an async signal to flag.
///
/// `sync` is excluded because the `.sync` suffix is a widespread Node.js
/// convention for the *synchronous* counterpart of an async API — `execa.sync()`,
/// `cross-spawn.sync()`, `glob.sync()` all return a plain value, never a Promise.
/// The name explicitly says "I am synchronous", so it is the opposite of an
/// async signal.
pub(super) const ASYNC_LOOKING_METHODS: &[&str] = &[
    "save", "load", "fetch", "query", "publish", "insert", "connect", "dispatch",
    "flush", "commit", "rollback", "run", "exec", "execute", "process", "handle",
];
