//! OxcCheck backend — flag `.removeChild()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["removeChild"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "removeChild" {
            return;
        }

        // Report at the property location (matching tree-sitter behaviour).
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.".into(),
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
    fn flags_remove_child() {
        let d = run_on("parent.removeChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("remove"));
    }


    #[test]
    fn flags_parent_node_remove_child() {
        let d = run_on("el.parentNode.removeChild(el);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_remove() {
        assert!(run_on("child.remove();").is_empty());
    }
}
