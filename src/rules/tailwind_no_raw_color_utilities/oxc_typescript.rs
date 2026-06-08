//! tailwind-no-raw-color-utilities OxcCheck backend — flag raw palette colors
//! in JSX `className`/`class` values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

const RAW_COLORS: &[&str] = &[
    "white", "black", "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber",
    "yellow", "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet",
    "purple", "fuchsia", "pink", "rose",
];

const COLOR_PREFIXES: &[&str] = &[
    "bg-",
    "text-",
    "border-",
    "ring-",
    "fill-",
    "stroke-",
    "from-",
    "to-",
    "via-",
    "divide-",
    "outline-",
    "decoration-",
    "placeholder-",
    "caret-",
    "accent-",
    "shadow-",
];

fn is_raw_color_class(token: &str) -> bool {
    for prefix in COLOR_PREFIXES {
        let Some(rest) = token.strip_prefix(prefix) else {
            continue;
        };
        if RAW_COLORS.contains(&rest) {
            return true;
        }
        if let Some((color, shade)) = rest.rsplit_once('-')
            && RAW_COLORS.contains(&color)
            && shade.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

fn strip_variants(token: &str) -> &str {
    token.rsplit(':').next().unwrap_or(token)
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
            if name.name != "className" && name.name != "class" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else { continue };
            let value = lit.value.as_str();
            let has_raw = value
                .split_whitespace()
                .any(|tok| is_raw_color_class(strip_variants(tok)));
            if has_raw {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Raw palette color utility in className — use semantic tokens \
                              (bg-background, text-foreground, bg-primary, …)."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_bg_white() {
        assert_eq!(
            run(r#"export const A = () => <div className="bg-white" />;"#).len(),
            1
        );
    }


    #[test]
    fn flags_text_gray_900() {
        assert_eq!(
            run(r#"export const A = () => <div className="text-gray-900" />;"#).len(),
            1
        );
    }


    #[test]
    fn flags_bg_blue_500() {
        assert_eq!(
            run(r#"export const A = () => <div className="p-4 bg-blue-500" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_semantic_tokens() {
        assert!(
            run(r#"export const A = () => <div className="bg-background text-foreground" />;"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_bg_primary() {
        assert!(
            run(
                r#"export const A = () => <div className="bg-primary text-primary-foreground" />;"#
            )
            .is_empty()
        );
    }
}
