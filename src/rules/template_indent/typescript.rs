//! template-indent — TS / JS / TSX backend.
//!
//! Walks `template_string` AST nodes and flags multi-line templates
//! whose content lines all share a common leading whitespace prefix
//! ≥ `min_indent` spaces — a sign the template inherited the
//! surrounding code's indentation instead of being explicitly dedented.
//!
//! Working against the AST (rather than scanning backticks textually)
//! avoids false positives on backticks appearing in regular strings,
//! comments, or markdown-in-prose.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["template_string"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return; };
    // Strip the leading and trailing backtick to get the literal body.
    let body = raw.strip_prefix('`').and_then(|s| s.strip_suffix('`'));
    let Some(body) = body else { return; };
    if !body.contains('\n') { return; }
    let min_indent = ctx.config.threshold("template-indent", "min_indent", ctx.lang);
    let Some(indent) = common_leading_whitespace(body) else { return; };
    if indent < min_indent { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "template-indent".into(),
        message: format!(
            "Template literal has {indent} spaces of common leading indentation \
             inherited from the surrounding code — strip it or use a dedent helper."
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

/// Compute the minimum leading-whitespace count across the template's
/// non-empty content lines.
///
/// Skips:
/// - the first physical line (opens immediately after the backtick, no
///   indent of its own)
/// - the last physical line (just whitespace before the closing
///   backtick, by convention)
/// - blank / whitespace-only lines
///
/// Returns `None` if no content line qualifies.
fn common_leading_whitespace(body: &str) -> Option<usize> {
    let lines: Vec<&str> = body.split('\n').collect();
    if lines.len() < 3 {
        return None;
    }
    let mut min_ws = usize::MAX;
    let mut has_content = false;
    // First line is what's after the opening backtick on its line —
    // skip it. Last line is what's before the closing backtick on its
    // line — also skip it.
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        has_content = true;
        let leading = line.len() - line.trim_start().len();
        min_ws = min_ws.min(leading);
    }
    if has_content { Some(min_ws) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_indented_template() {
        let src = r#"
function foo() {
    const html = `
        <div>
            <p>Hello</p>
        </div>
    `;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("common leading indentation"));
    }

    #[test]
    fn allows_template_without_excess_indent() {
        let src = r#"
const html = `
<div>
  <p>Hello</p>
</div>
`;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_line_template() {
        assert!(run("const x = `hello world`;").is_empty());
    }

    #[test]
    fn allows_template_with_minimal_indent() {
        let src = "const x = `\n  a\n  b\n`;\n";
        // 2 spaces is below the MIN_INDENT threshold.
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_template() {
        let src = r#"
if (true) {
    if (true) {
        const sql = `
            SELECT *
            FROM users
            WHERE id = 1
        `;
    }
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_backtick_in_string_literal() {
        // Single-quoted string containing a backtick — must NOT be
        // analysed as a template literal.
        let src = "const s = '`\n        line\n        line\n`';";
        assert!(run(src).is_empty());
    }
}
