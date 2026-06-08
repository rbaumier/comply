//! rust-env-var-unwrap-at-runtime — `env::var("X").unwrap()` outside
//! `fn main` and tests turns a missing environment variable into a runtime
//! panic deep inside business logic. Read it once at startup and pass the
//! value through.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-env-var-unwrap-at-runtime",
    description: "`env::var(\"X\").unwrap()` outside `main` / tests panics deep in business logic.",
    remediation: "Read the variable once at startup (in `main` or your \
                  config bootstrap) and pass the value through the call \
                  graph as a parameter. Inside business logic the failure \
                  mode of a missing env var should be a typed error, not \
                  a panic. `main` and tests are exempted.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "configuration"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
