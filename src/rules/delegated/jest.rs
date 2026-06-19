//! eslint-plugin-jest rules delegated to oxlint.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::backend::{Backend, PostFilter};
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        oxlint_delegate(
            RuleMeta {
                id: "jest-no-export",
                description:
                    "Don't `export` (or `module.exports`) from a file that contains tests.",
                remediation: "Remove the export from the test file. Exporting from a test file \
                              makes test runners treat it as a module others import, which can \
                              re-run the tests and leak helpers. Move any shared code into a \
                              separate non-test file and import it from there.",
                severity: Severity::Error,
                doc_url: None,
                categories: &["jest"],
                skip_in_test_dir: false,
                skip_in_relaxed_dir: false,
            },
            "jest/no-export",
            TS_FAMILY,
        ),
        consistent_test_it(),
    ]
}

/// `jest-consistent-test-it`, delegated to oxlint's `jest/consistent-test-it`
/// with a source-aware post-filter ([`ConsistentTestItFilter`]).
///
/// The rule enforces *consistency*: a file should pick one of the equivalent
/// `it()` / `test()` forms and stick to it. oxlint's default flags every
/// top-level `it()`, which fires on a file that uses `it()` exclusively even
/// though such a file is already consistent. The post-filter keeps a
/// diagnostic only when the file actually mixes both forms.
fn consistent_test_it() -> RuleDef {
    let meta = RuleMeta {
        id: "jest-consistent-test-it",
        description: "Within a single test file, use `it` and `test` consistently — pick one of \
                      the two equivalent forms instead of mixing them.",
        remediation: "Pick one form for this file and convert the others to match: use `it(...)` \
                      everywhere, or `test(...)` everywhere. Mixing `it(...)` and `test(...)` in \
                      the same file makes the test output read inconsistently.",
        severity: Severity::Warning,
        doc_url: None,
        categories: &["jest"],
        skip_in_test_dir: false,
        skip_in_relaxed_dir: false,
    };
    let filter: Arc<dyn PostFilter> = Arc::new(ConsistentTestItFilter);
    RuleDef {
        meta,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| {
                (
                    lang,
                    Backend::Oxlint {
                        rule: "jest/consistent-test-it",
                        post_filter: Some(Arc::clone(&filter)),
                    },
                )
            })
            .collect::<Vec<(Language, Backend)>>(),
    }
}

// ── jest/consistent-test-it post-filter ────────────────────────────────────
//
// The rule's purpose is consistency, not preferring one form. A file that uses
// `it()` exclusively (or `test()` exclusively) is already consistent, so a
// diagnostic on it is a false positive. Only a file that MIXES both forms is
// inconsistent and must be flagged.

struct ConsistentTestItFilter;

impl PostFilter for ConsistentTestItFilter {
    fn keep(&self, _diag: &Diagnostic, source: Option<&str>) -> bool {
        // Unreadable source → keep the diagnostic (conservative: don't silently
        // drop a real inconsistency just because we couldn't read the file).
        let Some(src) = source else {
            return true;
        };
        file_uses_it_call(src) && file_uses_test_call(src)
    }
}

/// True when `src` contains a call to the bare `it` test function (`it(...)`,
/// `it.each`, `it.only`, …).
fn file_uses_it_call(src: &str) -> bool {
    has_bare_call(src, "it")
}

/// True when `src` contains a call to the bare `test` test function
/// (`test(...)`, `test.each`, `test.only`, …).
fn file_uses_test_call(src: &str) -> bool {
    has_bare_call(src, "test")
}

/// True when `name` appears in `src` as a standalone identifier immediately
/// followed (ignoring whitespace) by `(` or `.` — i.e. a call or member access
/// on the bare function, such as `it(`, `it.each`, `test(`, `test.only`.
///
/// The match requires a word boundary before `name`: the preceding byte must be
/// start-of-file or a non-identifier character. This is what stops `latest(`
/// matching `test` and `commit(` / `submit(` / `omit(` / `await it` matching
/// `it`. ASCII-only boundary checks are sufficient here — JS identifiers may
/// contain non-ASCII characters, but a multibyte UTF-8 continuation byte is
/// never a boundary, so treating it as a non-boundary (identifier-like) byte is
/// correct and never produces a false match.
fn has_bare_call(src: &str, name: &str) -> bool {
    let bytes = src.as_bytes();
    src.match_indices(name).any(|(start, _)| {
        let end = start + name.len();
        let boundary_before = start == 0 || !is_ident_byte(bytes[start - 1]);
        boundary_before && next_non_ws_is_call_or_member(&bytes[end..])
    })
}

/// True when the first non-whitespace byte after the identifier is `(` (a call)
/// or `.` (member access like `.each` / `.only`). Trailing whitespace with no
/// such byte (e.g. the identifier is a bare reference) is not a call.
fn next_non_ws_is_call_or_member(rest: &[u8]) -> bool {
    matches!(
        rest.iter().find(|b| !b.is_ascii_whitespace()),
        Some(b'(') | Some(b'.')
    )
}

/// True for a byte that can appear inside a JS identifier (`[A-Za-z0-9_$]`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn it_call_is_detected() {
        assert!(file_uses_it_call("it('x', () => {})"));
        assert!(file_uses_it_call("it.each([[1]])('x', () => {})"));
        assert!(file_uses_it_call("it .only('x', () => {})"));
    }

    #[test]
    fn test_call_is_detected() {
        assert!(file_uses_test_call("test('x', () => {})"));
        assert!(file_uses_test_call("test.each([[1]])('x', () => {})"));
        assert!(file_uses_test_call("test.only('x', () => {})"));
    }

    #[test]
    fn substring_matches_do_not_trigger_it() {
        // `it` is a substring of these identifiers; none is a call to bare `it`.
        assert!(!file_uses_it_call("submit('x')"));
        assert!(!file_uses_it_call("commit('x')"));
        assert!(!file_uses_it_call("omit({ a: 1 })"));
        assert!(!file_uses_it_call("await something()"));
    }

    #[test]
    fn substring_matches_do_not_trigger_test() {
        // `test(` is the substring `latest(` ends with; not a call to `test`.
        assert!(!file_uses_test_call("latest('x')"));
        assert!(!file_uses_test_call("greatest(1, 2)"));
        assert!(!file_uses_test_call("const protest = retest()"));
    }

    #[test]
    fn bare_reference_without_call_is_not_a_call() {
        assert!(!file_uses_it_call("import { it } from 'vitest'"));
        assert!(!file_uses_test_call("export { test }"));
    }

    fn diag() -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(Path::new("foo.spec.ts")),
            line: 1,
            column: 1,
            rule_id: "jest-consistent-test-it".into(),
            message: "Enforce `test` and `it` usage conventions".into(),
            severity: Severity::Warning,
            span: None,
        }
    }

    #[test]
    fn it_only_file_is_suppressed() {
        // The ni shape: imports `it`, uses only `it()`.
        let src = "import { it } from 'vitest'\nit('a', f)\nit('b', g)\n";
        assert!(!ConsistentTestItFilter.keep(&diag(), Some(src)));
    }

    #[test]
    fn test_only_file_is_suppressed() {
        let src = "import { test } from 'vitest'\ntest('a', f)\ntest('b', g)\n";
        assert!(!ConsistentTestItFilter.keep(&diag(), Some(src)));
    }

    #[test]
    fn mixed_file_is_kept() {
        // Real inconsistency: both forms in the same file — still flagged.
        let src = "it('a', f)\ntest('b', g)\n";
        assert!(ConsistentTestItFilter.keep(&diag(), Some(src)));
    }

    #[test]
    fn unreadable_source_is_kept() {
        assert!(ConsistentTestItFilter.keep(&diag(), None));
    }
}
