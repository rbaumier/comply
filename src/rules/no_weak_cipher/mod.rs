//! no-weak-cipher — flag the use of weak symmetric ciphers
//! (DES, 3DES, RC2, RC4, Blowfish) in crypto APIs.
//!
//! Detection is **narrow by call context, loose by content**, which is
//! the shape SonarJS's `S5547` uses for the JS/TS equivalent:
//!
//! - **TypeScript / JavaScript**: walk `call_expression` nodes, match
//!   a callee whose trailing name is `createCipheriv` (Node.js's modern
//!   crypto cipher API), and check if the first string-literal argument
//!   starts with one of `bf`, `blowfish`, `des`, `rc2`, `rc4`.
//!
//! - **Rust**: walk `call_expression` nodes, match a
//!   `scoped_identifier` function of the form
//!   `[<path>::]Cipher::<weak_name>` where `<weak_name>` starts with a
//!   weak-cipher family prefix (`des`, `rc4`, `rc2`, `bf`, `blowfish`)
//!   followed by `_` or end-of-identifier. This matches the `openssl`
//!   crate's `openssl::symm::Cipher::des_ecb()` / `Cipher::rc4()` /
//!   etc. — Rust crypto libraries select the cipher by method name
//!   rather than by a string argument, which is why the TS backend's
//!   approach doesn't transfer directly.
//!
//! The previous implementation scanned every string literal in every
//! Rust file for cipher-like substrings, producing false positives on
//! unrelated strings (`"jsdoc-require-throws-description"` matched the
//! `-des` prefix inside `-description`). The new design cannot
//! false-positive on arbitrary strings because it never looks at
//! strings outside `createCipheriv(...)` arguments or at calls outside
//! `Cipher::<method>` shapes.
//!
//! Known gap: TS backend does not do constant propagation, so
//! `const ALGO = "des-cbc"; createCipheriv(ALGO, ...)` is not flagged.
//! SonarJS handles this via `getValueOfExpression`; we can add a
//! file-level binding scan later if needed.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-cipher",
    description: "Weak symmetric cipher (DES, RC2, RC4, Blowfish) used in a crypto API call.",
    remediation: "Use AES-256-GCM or ChaCha20-Poly1305 instead.",
    severity: Severity::Error,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S5547/javascript"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
