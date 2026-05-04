//! ts-bounded-recursive-generic OXC backend — flag recursive conditional/mapped
//! types that lack a depth accumulator parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };

        let name = alias.id.name.as_str();
        if name.is_empty() {
            return;
        }

        // Get the full source text of the type annotation to check for
        // conditional/mapped types and self-references.
        let ann_text =
            &ctx.source[alias.type_annotation.span().start as usize..alias.type_annotation.span().end as usize];

        // Must be a conditional or mapped type (heuristic: check text).
        let is_conditional_or_mapped =
            ann_text.contains(" extends ") || ann_text.contains("[") && ann_text.contains(" in ");
        if !is_conditional_or_mapped {
            return;
        }

        // Must reference itself.
        if !references_name(ann_text, name) {
            return;
        }

        // Must lack a depth parameter.
        if let Some(type_params) = &alias.type_parameters
            && has_depth_parameter(type_params, ctx.source) {
                return;
            }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Recursive type `{name}` has no depth parameter; add one to bound recursion."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Check if the type annotation text references the given name as a standalone
/// identifier (followed by `<` or non-alphanumeric).
fn references_name(text: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(name) {
        let abs = start + pos;
        let after = abs + name.len();
        // Check the character before is not alphanumeric/_
        let ok_before = abs == 0
            || !text.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && text.as_bytes()[abs - 1] != b'_';
        // Check the character after is not alphanumeric/_
        let ok_after = after >= text.len()
            || !text.as_bytes()[after].is_ascii_alphanumeric()
                && text.as_bytes()[after] != b'_';
        if ok_before && ok_after {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn has_depth_parameter(
    type_params: &oxc_ast::ast::TSTypeParameterDeclaration,
    source: &str,
) -> bool {
    for tp in &type_params.params {
        let text = &source[tp.span.start as usize..tp.span.end as usize];
        if text.contains("Depth") || text.contains("Count") {
            return true;
        }
        if text.contains("extends number") || text.contains("extends 0") {
            return true;
        }
    }
    false
}
