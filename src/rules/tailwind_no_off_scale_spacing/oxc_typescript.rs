//! OXC backend for tailwind-no-off-scale-spacing — flag spacing utilities
//! that fall outside the conventional 4/8pt scale in JSX className attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

const SPACING_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pb-", "pl-", "pr-", "ps-", "pe-", "m-", "mx-", "my-", "mt-",
    "mb-", "ml-", "mr-", "ms-", "me-", "gap-", "gap-x-", "gap-y-", "space-x-", "space-y-",
];

const ON_SCALE: &[&str] = &[
    "0", "px", "0.5", "1", "1.5", "2", "2.5", "3", "3.5", "4", "6", "8", "10", "12", "14", "16",
    "20", "24", "28", "32", "36", "40", "44", "48", "52", "56", "60", "64", "72", "80", "96",
];

fn is_off_scale(base: &str) -> bool {
    for prefix in SPACING_PREFIXES {
        let Some(rest) = base.strip_prefix(prefix) else {
            continue;
        };
        if rest.starts_with('-') || rest == "auto" || rest == "full" || rest.starts_with('[') {
            return false;
        }
        return !ON_SCALE.contains(&rest);
    }
    false
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

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "className" && name.name.as_str() != "class" {
                continue;
            }

            // Extract string value from the attribute.
            let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let value = lit.value.as_str();

            let off = value.split_whitespace().any(|tok| {
                let base = tok.rsplit(':').next().unwrap_or(tok);
                is_off_scale(base)
            });
            if !off {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Off-scale spacing value — snap to the 4pt grid (\u{2026}4, 6, 8, 10, 12, 16\u{2026}) to stay on the design system.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
