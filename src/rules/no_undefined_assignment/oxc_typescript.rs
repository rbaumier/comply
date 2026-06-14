//! no-undefined-assignment oxc backend — flag `= undefined` assignments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentOperator, AssignmentTarget, Expression};
use std::sync::Arc;

pub struct Check;

/// True for `<expr>.current = undefined` — the React ref-clearing idiom.
///
/// A `MutableRefObject<T>` has `current: T | null` (never `T | undefined`), so
/// `delete ref.current` is a TypeScript error and would break the ref contract.
/// Assigning `undefined` is the intended way to mark the ref as holding no value.
fn is_ref_current_target(target: &AssignmentTarget) -> bool {
    matches!(
        target,
        AssignmentTarget::StaticMemberExpression(member)
            if member.property.name.as_str() == "current"
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (is_undefined, span_start) = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                let is_undef =
                    matches!(init, Expression::Identifier(id) if id.name.as_str() == "undefined");
                (is_undef, decl.span.start)
            }
            AstKind::AssignmentExpression(assign) => {
                let is_undef = matches!(
                    &assign.right,
                    Expression::Identifier(id) if id.name.as_str() == "undefined"
                );
                if assign.operator == AssignmentOperator::Assign
                    && is_ref_current_target(&assign.left)
                {
                    return;
                }
                (is_undef, assign.span.start)
            }
            _ => return,
        };

        if !is_undefined {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not assign `undefined` \u{2014} use `let x;` or `delete obj.prop` instead."
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
    fn allows_ref_current_undefined() {
        assert!(run_on("jointRef.current = undefined;").is_empty());
        assert!(run_on("someRef.current = undefined;").is_empty());
    }

    #[test]
    fn flags_let_undefined() {
        assert_eq!(run_on("let x = undefined;").len(), 1);
    }

    #[test]
    fn flags_plain_identifier_reassignment() {
        assert_eq!(run_on("instance = undefined;").len(), 1);
    }

    #[test]
    fn flags_member_property_not_current() {
        assert_eq!(run_on("obj.value = undefined;").len(), 1);
    }
}
