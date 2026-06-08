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

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_bg_blue_500() {
        assert_eq!(
            run(r#"const x = <div className="bg-blue-500">x</div>;"#).len(),
            1
        );
    }


    #[test]
    fn flags_text_gray_600() {
        assert_eq!(
            run(r#"const x = <span className="text-gray-600">x</span>;"#).len(),
            1
        );
    }


    #[test]
    fn flags_mixed_with_other_utilities() {
        assert_eq!(
            run(r#"const x = <div className="p-4 bg-red-100 rounded">x</div>;"#).len(),
            1
        );
    }


    #[test]
    fn allows_semantic_tokens() {
        assert!(
            run(r#"const x = <div className="bg-primary text-muted-foreground">x</div>;"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_non_color_utilities() {
        assert!(run(r#"const x = <div className="p-4 rounded-md flex">x</div>;"#).is_empty());
    }
}
