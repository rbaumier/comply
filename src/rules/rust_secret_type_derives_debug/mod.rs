//! rust-secret-type-derives-debug — a struct whose name says it carries a
//! secret must not get its textual rendering from a `derive`.
//!
//! `#[derive(Debug)]` prints every field verbatim, and the derived output
//! reaches places nobody reviews: `tracing` spans, `dbg!`, a `.unwrap()`
//! panic message, an `anyhow` error chain. The same holds for
//! `derive_more::Display` and for a `#[derive(Serialize)]` that serializes
//! the secret field unprotected.
//!
//! The rule fires on a struct whose name carries a secret-bearing token
//! (`Secret`, `Password`, `ApiKey`, `PrivateKey`, `AccessToken`, …) and
//! that still holds an unmasked secret field. A struct already built on
//! `secrecy`/`zeroize`, or with a hand-written `Debug`/`Display` impl, is
//! left alone.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-secret-type-derives-debug",
    description: "A secret-carrying struct that derives `Debug`/`Display`/`Serialize` leaks the secret into logs, spans and panic messages.",
    remediation: "Write the impl by hand and mask the value — \
                  `impl fmt::Debug for ApiKey { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(\"ApiKey(****)\") } }` \
                  — or hold the value in `secrecy::SecretString` / `SecretBox<T>`, whose own `Debug` prints `[REDACTED]`. \
                  For a derived `Serialize`, mark the secret field `#[serde(skip)]` or give it a redacting `serialize_with`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
