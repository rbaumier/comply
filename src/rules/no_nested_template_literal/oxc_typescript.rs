//! no-nested-template-literal OXC backend — flag template literals that
//! contain another template literal inside an interpolation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(_tpl) = node.kind() else {
            return;
        };

        // Check if any ancestor is also a TemplateLiteral — if so, the
        // *ancestor* is the one we report, so skip this node to avoid
        // double-reporting.
        for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
            if matches!(ancestor.kind(), AstKind::TemplateLiteral(_)) {
                return;
            }
        }

        // Now check if any descendant template literal exists (i.e. this
        // template literal has a nested one inside its expressions).
        if !has_nested_template(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, _tpl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested template literal \u{2014} extract the inner template to a named variable."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn has_nested_template<'a>(
    parent: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Walk all nodes and check if any TemplateLiteral has this node as
    // an ancestor (i.e. is nested inside it).
    for child in semantic.nodes().iter() {
        if child.id() == parent.id() {
            continue;
        }
        if !matches!(child.kind(), AstKind::TemplateLiteral(_)) {
            continue;
        }
        // Check this child is a descendant of parent.
        for anc in semantic.nodes().ancestors(child.id()).skip(1) {
            if anc.id() == parent.id() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_single_interpolation() {
        assert!(run(r#"const msg = `Hello ${name}`;"#).is_empty());
    }


    #[test]
    fn allows_no_interpolation() {
        assert!(run(r#"const msg = `plain string`;"#).is_empty());
    }


    #[test]
    fn allows_multiple_interpolations_in_same_template() {
        // Regression: previously flagged because the check counted
        // `${` occurrences. Two interpolations in the same template
        // literal are not nesting — the tree-sitter AST has a single
        // `template_string` node with two `template_substitution`
        // children.
        assert!(
            run(r#"const url = baseUrl + `${baseUrl}/api/v1/subscriptions/${subscriptionId}`;"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_function_call_in_interpolation() {
        // A function call inside `${}` is not a nested template —
        // just an expression that happens to produce a string.
        assert!(run(r#"const msg = `id: ${String(id)}`;"#).is_empty());
    }
}
