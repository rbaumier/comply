//! jsdoc/require-rejects — async functions that can reject must declare `@rejects`.
//!
//! Heuristic: flag a JSDoc block whose following code is an `async` function
//! containing `Promise.reject(` or a `throw` statement, when neither
//! `@rejects` nor `@throws` is present. This is intentionally narrow — we
//! don't flag every async function, only those with a visible rejection path.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

fn is_async_fn(code: &str) -> bool {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first_line.starts_with("async ")
        || first_line.starts_with("export async ")
        || first_line.starts_with("export default async ")
        || first_line.contains(" async ")
}

fn has_rejection_path(code: &str) -> bool {
    code.contains("Promise.reject(") || code.contains("throw ")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "comment" { return; }
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let tags = parse_tags(&block.content);
        if has_tag(&tags, "rejects") || has_tag(&tags, "throws") {
            continue;
        }
        // Pull the following code from the file source (not the node
        // text) since the AST node only contains the comment.
        let code = following_code(ctx.source, text);
        if !is_async_fn(code) {
            continue;
        }
        if !has_rejection_path(code) {
            continue;
        }
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: block.start_line + 1 + line_offset,
            column: 1,
            rule_id: "jsdoc/require-rejects".into(),
            message: "Async function may reject — document it with `@rejects {ErrorType} when ...`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_async_fn_with_throw_and_no_rejects_tag() {
        let src = "/**\n * does things\n */\nasync function f() { throw new Error('x'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_async_with_promise_reject() {
        let src = "/**\n * does things\n */\nasync function f() { return Promise.reject(new Error('x')); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_fn_with_rejects_tag() {
        let src = "/**\n * does things\n * @rejects {Error} when broken\n */\nasync function f() { throw new Error('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sync_fn_with_throw() {
        let src = "/**\n * does things\n */\nfunction f() { throw new Error('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_async_fn_with_no_rejection_path() {
        let src = "/**\n * ok\n */\nasync function f() { return 1; }";
        assert!(run(src).is_empty());
    }
}
