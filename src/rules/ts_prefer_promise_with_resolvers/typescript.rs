//! Flag `new Promise(` patterns. We only care about the literal call to the
//! `Promise` constructor — sub-classes (e.g. `class P extends Promise`) are
//! out of scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_word_boundary(prev: Option<u8>) -> bool {
    match prev {
        None => true,
        Some(b) => !b.is_ascii_alphanumeric() && b != b'_' && b != b'$' && b != b'.',
    }
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new Promise("])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let bytes = ctx.source.as_bytes();
        let needle = b"new Promise(";
        let mut diagnostics = Vec::new();
        let mut line: usize = 1;
        let mut col: usize = 1;
        let mut i: usize = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\n' {
                line += 1;
                col = 1;
                i += 1;
                continue;
            }
            if i + needle.len() <= bytes.len()
                && &bytes[i..i + needle.len()] == needle
                && is_word_boundary(if i == 0 { None } else { Some(bytes[i - 1]) })
            {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "`new Promise(...)` is verbose — prefer \
                              `Promise.withResolvers()` to get `{ promise, resolve, reject }` \
                              without nesting code in an executor closure."
                        .to_string(),
                    severity: Severity::Warning,
                    span: None,
                });
                i += needle.len();
                col += needle.len();
                continue;
            }
            i += 1;
            col += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_new_promise_constructor() {
        let src = "const p = new Promise((resolve, reject) => resolve(1));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_constructors() {
        let src = "const a = new Promise((r) => r(1));\nconst b = new Promise((r) => r(2));";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_promise_with_resolvers() {
        let src = "const { promise, resolve } = Promise.withResolvers();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_resolve_static() {
        let src = "const p = Promise.resolve(42);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_identifier_named_promise_dot() {
        // `obj.Promise(` shouldn't match because `new` isn't there.
        let src = "obj.Promise(42);";
        assert!(run(src).is_empty());
    }
}
