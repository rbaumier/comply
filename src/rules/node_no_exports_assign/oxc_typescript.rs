//! OXC backend for node-no-exports-assign.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::{AssignmentTarget, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["exports"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Only flag `exports = ...` (direct assignment to bare identifier `exports`).
        let AssignmentTarget::AssignmentTargetIdentifier(ident) = &assign.left else {
            return;
        };
        if ident.name.as_str() != "exports" {
            return;
        }

        // Allow `module.exports = exports = {}` pattern:
        // if parent is also an assignment whose left is `module.exports`, skip.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::AssignmentExpression(parent_assign) = parent.kind()
            && is_module_exports_target(&parent_assign.left) {
                return;
            }

        // Allow `exports = module.exports = {}` pattern.
        if let Expression::AssignmentExpression(ref right) = assign.right
            && is_module_exports_target(&right.left) {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected assignment to `exports` variable. Use `module.exports` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_module_exports_target(target: &AssignmentTarget) -> bool {
    match target {
        AssignmentTarget::StaticMemberExpression(mem) => {
            if mem.property.name.as_str() != "exports" {
                return false;
            }
            matches!(&mem.object, Expression::Identifier(id) if id.name.as_str() == "module")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_exports_assignment() {
        let d = run_on("exports = { foo: 1 };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }


    #[test]
    fn allows_module_exports() {
        assert!(run_on("module.exports = { foo: 1 };").is_empty());
    }


    #[test]
    fn allows_exports_property() {
        // `exports.foo = 1` is setting a property, not reassigning `exports` itself.
        assert!(run_on("exports.foo = 1;").is_empty());
    }
}
