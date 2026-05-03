//! shadcn-no-raw-tailwind-colors OxcCheck backend — flag raw Tailwind color
//! utilities in JSX `className` values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

const COLOR_PREFIXES: &[&str] = &[
    "bg",
    "text",
    "border",
    "ring",
    "fill",
    "stroke",
    "from",
    "to",
    "via",
    "divide",
    "outline",
    "accent",
    "caret",
    "placeholder",
    "shadow",
    "decoration",
];

const COLORS: &[&str] = &[
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose",
];

fn is_raw_color_class(class: &str) -> bool {
    let utility = class.rsplit(':').next().unwrap_or(class);
    let utility = utility.trim_start_matches('!').trim_start_matches('-');

    let mut parts = utility.split('-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    if !COLOR_PREFIXES.contains(&prefix) {
        return false;
    }
    let Some(color) = parts.next() else {
        return false;
    };
    if !COLORS.contains(&color) {
        return false;
    }
    let Some(shade) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    shade.len() >= 2 && shade.len() <= 3 && shade.chars().all(|c| c.is_ascii_digit())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name != "className" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else { continue };
            let value = lit.value.as_str();
            if value.split_ascii_whitespace().any(is_raw_color_class) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`className` uses raw Tailwind colors — switch to shadcn semantic \
                              tokens (`bg-primary`, `text-muted-foreground`, …)."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
