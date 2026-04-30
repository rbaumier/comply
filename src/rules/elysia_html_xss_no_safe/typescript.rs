//! elysia-html-xss-no-safe backend — flag JSX expressions with user input lacking `safe`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true when `text` contains `body`, `query`, or `params` as a
/// standalone identifier — preceded by `{`, `.`, or whitespace and followed by
/// `.`, `}`, `[`, `,`, or whitespace. This prevents substring false positives
/// such as `{bodyClass}` or `{bodyWeight}`.
fn mentions_dangerous_identifier(text: &str) -> bool {
    const NAMES: &[&str] = &["body", "query", "params"];
    let bytes = text.as_bytes();
    for name in NAMES {
        let nb = name.as_bytes();
        let mut i = 0;
        while i + nb.len() <= bytes.len() {
            if &bytes[i..i + nb.len()] == nb {
                let before_ok =
                    i == 0 || matches!(bytes[i - 1], b'{' | b'.' | b' ' | b'\t' | b'\n' | b'\r');
                let after_idx = i + nb.len();
                let after_ok = after_idx == bytes.len()
                    || matches!(
                        bytes[after_idx],
                        b'.' | b'}' | b'[' | b',' | b' ' | b'\t' | b'\n' | b'\r'
                    );
                if before_ok && after_ok {
                    return true;
                }
            }
            i += 1;
        }
    }
    false
}

crate::ast_check! { on ["jsx_element"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // Only inspect direct `jsx_expression` children so nested elements don't
    // trigger duplicate diagnostics for the same interpolation, and so substring
    // matches like `{bodyClass}` are filtered by `mentions_dangerous_identifier`.
    let mut has_dangerous_expr = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_expression" {
            continue;
        }
        let expr_text = child.utf8_text(source).unwrap_or("");
        if mentions_dangerous_identifier(expr_text) {
            has_dangerous_expr = true;
            break;
        }
    }
    if !has_dangerous_expr {
        return;
    }

    // Look for `safe` attribute on the opening element.
    let mut has_safe = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_opening_element" {
            continue;
        }
        let mut c2 = child.walk();
        for attr in child.children(&mut c2) {
            if attr.kind() == "jsx_attribute" {
                let attr_text = attr.utf8_text(source).unwrap_or("");
                let name = attr_text.split('=').next().unwrap_or("").trim();
                if name == "safe" {
                    has_safe = true;
                }
            }
        }
    }

    if has_safe {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-html-xss-no-safe".into(),
        message: "JSX element interpolates user input without `safe` — add the `safe` attribute to escape it.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_jsx_with_user_input_no_safe() {
        let src = "import { html } from '@elysiajs/html';\nconst v = <div>{body.name}</div>;";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_jsx_with_safe() {
        let src = "import { html } from '@elysiajs/html';\nconst v = <div safe>{body.name}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const v = <div>{body.name}</div>;";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_substring_match_bodyclass() {
        let src = "import { html } from '@elysiajs/html';\nconst bodyClass = 'x';\nconst v = <div>{bodyClass}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn fires_once_for_nested_jsx() {
        let src = "import { html } from '@elysiajs/html';\nconst v = <div><span>{body.name}</span></div>;";
        assert_eq!(run_on(src).len(), 1);
    }
}
