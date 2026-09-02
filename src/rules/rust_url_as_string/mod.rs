//! rust-url-as-string — a URL kept as `String`/`&str` and then patched by hand.
//!
//! The tell is the string surgery that follows: trimming a trailing `/`,
//! `format!`-ing a path onto a base, checking `starts_with("http")`. Each of
//! those is a rule of URL syntax re-implemented at a call site, and each of
//! them is wrong for some input the parser would have handled.
//!
//! The fix is to parse once, at the boundary that receives the string (clap,
//! a config file, an env var), and carry a `url::Url` from there on.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-url-as-string",
    description: "A URL held as `String`/`&str` and then patched with string surgery (trailing-slash trimming, `format!` joins) should be a parsed `url::Url`.",
    remediation: "Parse the string into `url::Url` at the boundary that receives it — \
                  clap (`#[arg(value_parser = clap::value_parser!(Url))]`), config deserialization, an env var — \
                  so a malformed URL fails there instead of halfway through a request. \
                  Build sub-paths with `Url::join` instead of `format!`. \
                  Watch the base: `join` resolves against the last path segment, so \
                  `\"https://h/api\".join(\"users\")` yields `/users` while `\"https://h/api/\".join(\"users\")` yields `/api/users` — \
                  keep the trailing slash on a base you mean to extend.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
