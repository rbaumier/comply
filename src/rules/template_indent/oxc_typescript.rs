use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };
        // Only check simple template literals (no expressions / substitutions).
        if !tpl.expressions.is_empty() {
            return;
        }
        if tpl.quasis.len() != 1 {
            return;
        }
        let body = tpl.quasis[0].value.raw.as_str();
        if !body.contains('\n') {
            return;
        }
        let min_indent = ctx.config.threshold("template-indent", "min_indent", ctx.lang);
        let Some(indent) = common_leading_whitespace(body) else {
            return;
        };
        if indent < min_indent {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Template literal has {indent} spaces of common leading indentation \
                 inherited from the surrounding code — strip it or use a dedent helper."
            ),
            severity: super::META.severity,
            span: Some((tpl.span.start as usize, (tpl.span.end - tpl.span.start) as usize)),
        });
    }
}

/// Compute the minimum leading-whitespace count across the template's
/// non-empty content lines.
///
/// Skips the first physical line (opens immediately after the backtick)
/// and the last physical line (just whitespace before the closing backtick).
fn common_leading_whitespace(body: &str) -> Option<usize> {
    let lines: Vec<&str> = body.split('\n').collect();
    if lines.len() < 3 {
        return None;
    }
    let mut min_ws = usize::MAX;
    let mut has_content = false;
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        has_content = true;
        let leading = line.len() - line.trim_start().len();
        min_ws = min_ws.min(leading);
    }
    if has_content {
        Some(min_ws)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
        let src = "const s = '`\n        line\n        line\n`';";
        assert!(run(src).is_empty());
    }
}
