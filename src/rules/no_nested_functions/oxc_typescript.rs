//! no-nested-functions oxc backend — flag function declarations nested 3+ levels deep.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nesting_boundary(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    match node.kind() {
        AstKind::Function(_) => {
            let parent = semantic.nodes().parent_node(node.id());
            !matches!(parent.kind(), AstKind::CallExpression(_))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Count nesting boundaries among ancestors (skip self).
        // Arrow functions passed as call arguments are not counted.
        let mut depth = 0usize;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            if is_nesting_boundary(ancestor, semantic) {
                depth += 1;
            }
        }
        if depth < 2 {
            return;
        }
        let span = match node.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Function declared at nesting depth {} — extract to module scope.",
                depth
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_deeply_nested_named_functions() {
        let src = "function a() { function b() { function c() {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_framework_callback_nesting() {
        let src = r#"
app.get("/path", (ctx) => {
    db.query((rows) => {
        rows.map((r) => r.id);
    });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_test_describe_it_nesting() {
        let src = r#"
describe("suite", () => {
    it("test", () => {
        expect(run(() => {})).toBe(true);
    });
});
"#;
        assert!(run_on(src).is_empty());
    }
}
