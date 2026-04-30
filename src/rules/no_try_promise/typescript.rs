//! no-try-promise backend — flag unawaited promises inside try blocks.

use crate::diagnostic::{Diagnostic, Severity};

/// Patterns that typically return a promise.
const PROMISE_PATTERNS: &[&str] = &[
    ".then(",
    "fetch(",
    "axios(",
    "axios.get(",
    "axios.post(",
    "axios.put(",
    "axios.delete(",
    "axios.patch(",
];

/// Returns true if the text contains a promise-returning call without `await`.
fn has_unawaited_promise(text: &str) -> bool {
    if text.contains("await ") || text.contains("await(") {
        return false;
    }
    PROMISE_PATTERNS.iter().any(|p| text.contains(p))
}

crate::ast_check! { on ["try_statement"] prefilter = ["try"] => |node, source, ctx, diagnostics|
    // We only care about expression_statement nodes inside try blocks.
    // Get the try body block.
    let Some(body) = node.child_by_field_name("body") else { return };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        let text = child.utf8_text(source).unwrap_or("");
        if has_unawaited_promise(text) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-try-promise".into(),
                message: "Promise inside try/catch without `await` \u{2014} rejection won't be caught.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_fetch_without_await_in_try() {
        let d = crate::rules::test_helpers::run_ts(
            r#"
try {
    const res = fetch("/api");
} catch (e) {}
"#,
            &Check,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-promise");
    }

    #[test]
    fn flags_then_without_await_in_try() {
        let d = crate::rules::test_helpers::run_ts(
            r#"
try {
    getData().then(r => r.json());
} catch (e) {}
"#,
            &Check,
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_awaited_fetch_in_try() {
        let d = crate::rules::test_helpers::run_ts(
            r#"
try {
    const res = await fetch("/api");
} catch (e) {}
"#,
            &Check,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_fetch_outside_try() {
        let d = crate::rules::test_helpers::run_ts(r#"const res = fetch("/api");"#, &Check);
        assert!(d.is_empty());
    }
}
