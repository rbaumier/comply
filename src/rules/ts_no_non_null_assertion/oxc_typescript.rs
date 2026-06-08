//! ts-no-non-null-assertion oxc backend — flag every `value!` postfix operator.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Extract the method name of the innermost call in a member-access chain.
/// `document.getElementById('root')!` → `Some("getElementById")`
/// `files.item(0)!` → `Some("item")`
fn base_callee_property<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StaticMemberExpression(m) => base_callee_property(&m.object),
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(m) = &call.callee {
                Some(m.property.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True when the node is nested inside an `if` statement (stops at function boundaries).
fn is_inside_if_block(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::IfStatement(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSNonNullExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else { return };

        let prop = base_callee_property(&expr.expression);

        // DOM queries always return the typed element-or-null; existence is guaranteed
        // by the surrounding HTML structure — the assertion is the only safe option.
        if matches!(prop, Some("getElementById" | "querySelector")) {
            return;
        }

        // `.item()` returns T|null regardless of bounds; suppress only when a
        // containing `if` already checked the collection length/truthiness.
        if matches!(prop, Some("item")) && is_inside_if_block(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid non-null assertions (`!`) — they silence the type \
                      checker. Narrow the type or use optional chaining instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_non_null_on_identifier() {
        let d = run_on("const x = value!;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_null_on_member() {
        let d = run_on("const x = obj.foo!;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_null_in_call() {
        let d = run_on("fn(value!);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_getelementbyid_assertion() {
        assert!(run_on("document.getElementById('root')!;").is_empty());
    }

    #[test]
    fn allows_queryselector_assertion() {
        assert!(run_on("document.querySelector('.cls')!;").is_empty());
    }

    #[test]
    fn allows_getelementbyid_then_member() {
        assert!(run_on("document.getElementById('id')!.innerHTML;").is_empty());
    }

    #[test]
    fn allows_item_call_inside_if() {
        assert!(run_on("if (files.length > 0) { const x = files.item(0)!; }").is_empty());
    }

    #[test]
    fn still_flags_item_outside_if() {
        let d = run_on("const x = files.item(0)!;");
        assert_eq!(d.len(), 1);
    }
}
