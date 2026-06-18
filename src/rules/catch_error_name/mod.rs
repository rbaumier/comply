//! catch-error-name — enforce the `error` name for `try { … } catch (…)`
//! parameters.
//!
//! ## Rationale
//!
//! Ported from `eslint-plugin-unicorn/catch-error-name`. Every catch in
//! the codebase uses the same binding name, so a grep for `error.` or
//! `error instanceof` finds every error-handling site without knowing
//! whether the local author typed `e`, `err`, `ex`, or `exception`.
//!
//! ## Language coverage
//!
//! **TypeScript / JavaScript / TSX**: handled by the `typescript`
//! backend. Flags `catch_clause` parameters that are simple identifiers
//! not matching the allowlist below.
//!
//! **Vue SFC (`<script>` and `<script setup>`)**: handled by the `vue`
//! backend. Extracts each `<script>` block via `vue_sfc::extract_scripts`,
//! re-parses its body with the TypeScript grammar, and runs the same
//! catch-parameter check. Diagnostic coordinates are translated back
//! to the Vue file.
//!
//! **Rust**: not covered. `Err(e)` is idiomatic in `match` / `if let`
//! and `.map_err(|e| …)`, and clippy explicitly does not flag it.
//! Forcing a unicorn/TS convention onto Rust pattern matching would
//! produce enormous noise without improving error handling.
//!
//! ## Allowed names
//!
//! - `_` — the parameter is unused. `catch (_) { return fallback; }`
//!   is a legitimate shape.
//! - `error` — the canonical name.
//! - `cause` — the caught error is forwarded as the `cause` of a new
//!   error: `catch (cause) { throw new Error(msg, { cause }); }`. The
//!   ES2022 `Error.cause` shorthand requires the binding to be named
//!   `cause`, so this name communicates intent rather than obscuring it.
//! - `*error` / `*Error` — suffix match for disambiguated bindings
//!   like `networkError`, `parseError`, `httpError`, `readError`. The
//!   shape `innerError` is allowed for nested try/catch where the
//!   outer binding is already `error`.
//!
//! Everything else (`e`, `err`, `ex`, `exception`, `caughtException`,
//! `failure`, …) is flagged.

mod oxc_typescript;
mod oxc_vue;
#[cfg(test)]
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "catch-error-name",
    description: "The catch parameter should be named `error`.",
    remediation: "Rename the catch parameter to `error` (or a suffixed \
                  variant like `parseError` when disambiguating nested \
                  catches). Use `_` if the parameter is unused.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/catch-error-name.md",
    ),
    categories: &["unicorn"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// The canonical name the rule wants on the catch parameter.
pub(super) const EXPECTED: &str = "error";

/// Whether a catch-parameter identifier is acceptable.
///
/// Allowed shapes:
/// - `_` — deliberately-unused parameter.
/// - `error` — the canonical name.
/// - `cause` — forwarded as the `cause` of a new error via the ES2022
///   `Error.cause` shorthand (`throw new Error(msg, { cause })`).
/// - `*error` / `*Error` — suffix forms like `networkError`, `parseError`,
///   `innerError` (used to disambiguate nested catches that would
///   otherwise shadow the outer `error`).
pub(super) fn is_acceptable_name(name: &str) -> bool {
    name == "_"
        || name == EXPECTED
        || name == "cause"
        || name.ends_with(EXPECTED)
        || name.ends_with("Error")
}

pub fn register() -> RuleDef {
    // Built manually (instead of `register_ts_family!`) so we can attach
    // a Vue backend on top of the TS/JS/TSX set. The Vue backend
    // re-parses `<script>` bodies with the TypeScript grammar and runs
    // the same catch-parameter check.
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check))),
        ],
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn accepts_canonical_error() {
        assert!(is_acceptable_name("error"));
    }

    #[test]
    fn accepts_underscore() {
        assert!(is_acceptable_name("_"));
    }

    #[test]
    fn accepts_cause() {
        assert!(is_acceptable_name("cause"));
    }

    #[test]
    fn accepts_suffixed_lower() {
        assert!(is_acceptable_name("networkerror"));
    }

    #[test]
    fn accepts_suffixed_upper() {
        assert!(is_acceptable_name("networkError"));
    }

    #[test]
    fn rejects_single_letter() {
        assert!(!is_acceptable_name("e"));
    }

    #[test]
    fn rejects_err() {
        assert!(!is_acceptable_name("err"));
    }

    #[test]
    fn rejects_ex() {
        assert!(!is_acceptable_name("ex"));
    }
}
