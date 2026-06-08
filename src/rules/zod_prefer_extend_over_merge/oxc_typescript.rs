//! zod-prefer-extend-over-merge OXC backend — flag `.merge()` on zod-looking receivers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk down the receiver expression looking for a `z` root identifier.
fn receiver_has_zod_root(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "z",
        Expression::StaticMemberExpression(member) => receiver_has_zod_root(&member.object),
        Expression::CallExpression(call) => receiver_has_zod_root(&call.callee),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["merge"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property "merge"
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "merge" {
            return;
        }

        // Check if receiver is zod-rooted or ends with "Schema"
        let hit = receiver_has_zod_root(&member.object) || {
            if let Expression::Identifier(id) = &member.object {
                id.name.as_str().ends_with("Schema")
            } else {
                false
            }
        };
        if !hit {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.merge()` is removed in Zod v4 — use `.extend(other.shape)` \
                      to combine object schemas."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_merge_on_z_object() {
        assert_eq!(
            run("const S = z.object({ a: z.string() }).merge(Other);").len(),
            1
        );
    }


    #[test]
    fn flags_merge_on_schema_variable() {
        assert_eq!(run("const S = UserSchema.merge(AdminSchema);").len(), 1);
    }


    #[test]
    fn allows_extend() {
        assert!(run("const S = UserSchema.extend({ role: z.string() });").is_empty());
    }


    #[test]
    fn ignores_unrelated_merge() {
        assert!(run("const r = _.merge(a, b);").is_empty());
    }
}
