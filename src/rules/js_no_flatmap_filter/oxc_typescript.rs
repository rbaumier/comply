//! js-no-flatmap-filter OXC backend — flag `.flatMap(...).filter(...)` chains.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["flatMap"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.filter(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "filter" {
            return;
        }

        // Receiver must be a call expression with `.flatMap(...)`.
        let Expression::CallExpression(inner_call) = &member.object else {
            return;
        };
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        if inner_member.property.name.as_str() != "flatMap" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.flatMap().filter()` iterates twice — return `[]` from the `flatMap` \
                      callback to filter and transform in a single pass."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_flatmap_filter_chain() {
        let diags = run(r#"const r = xs.flatMap(x => x.children).filter(c => c.active);"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_flatmap_filter_boolean() {
        let diags = run(r#"const r = xs.flatMap(x => x.tags).filter(Boolean);"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_flatmap_filter_multiline() {
        let diags = run(r#"
const result = items
    .flatMap(item => item.children)
    .filter(child => child.visible);
"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_flatmap_alone() {
        assert!(run(r#"const r = xs.flatMap(x => x.children);"#).is_empty());
    }


    #[test]
    fn allows_filter_alone() {
        assert!(run(r#"const r = xs.filter(x => x.active);"#).is_empty());
    }


    #[test]
    fn allows_map_filter_chain() {
        assert!(run(r#"const r = xs.map(x => x.id).filter(Boolean);"#).is_empty());
    }
}
