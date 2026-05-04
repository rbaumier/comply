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
