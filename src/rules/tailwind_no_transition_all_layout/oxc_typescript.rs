//! tailwind-no-transition-all-layout OXC backend — forbid `transition-all`
//! with layout properties in JSX className.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const LAYOUT_PREFIXES: &[&str] = &[
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-", "top-", "left-", "right-", "bottom-",
    "inset-",
];

fn is_layout_utility(tok: &str) -> bool {
    let base = tok.rsplit(':').next().unwrap_or(tok);
    LAYOUT_PREFIXES.iter().any(|p| {
        let Some(rest) = base.strip_prefix(p) else {
            return false;
        };
        !rest.is_empty()
            && (rest == "full"
                || rest == "screen"
                || rest == "auto"
                || rest.starts_with('[')
                || rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transition-all", "transition"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else { return };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let value = lit.value.as_str();

        let tokens: Vec<&str> = value.split_whitespace().collect();
        let has_transition_all = tokens.iter().any(|t| {
            let base = t.rsplit(':').next().unwrap_or(t);
            base == "transition-all" || base == "transition"
        });
        if !has_transition_all {
            return;
        }

        let has_layout = tokens.iter().any(|t| is_layout_utility(t));
        if !has_layout {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`transition-all` combined with layout utilities (w-/h-/top-/left-/\u{2026}) triggers layout on every frame. Use `transition-transform` + `translate-*` or `transition-opacity` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
