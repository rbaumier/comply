//! shadcn-no-manual-dark-overrides oxc backend.
//!
//! Flag `className` containing `dark:<prefix>-<color>-<shade>` paired
//! with a non-dark light counterpart.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const COLOR_PREFIXES: &[&str] = &[
    "bg", "text", "border", "ring", "fill", "stroke", "from", "to", "via", "divide", "outline",
    "accent", "caret", "placeholder", "shadow", "decoration",
];

const COLORS: &[&str] = &[
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose", "white", "black",
];

fn dark_raw_color_prefix(class: &str) -> Option<&'static str> {
    let segments: Vec<&str> = class.split(':').collect();
    if segments.len() < 2 {
        return None;
    }
    if !segments
        .iter()
        .take(segments.len() - 1)
        .any(|s| *s == "dark")
    {
        return None;
    }
    let utility = segments.last().copied().unwrap_or("");
    let utility = utility.trim_start_matches('!').trim_start_matches('-');
    let mut parts = utility.split('-');
    let prefix = parts.next()?;
    let matched_prefix = COLOR_PREFIXES.iter().find(|p| **p == prefix)?;
    let color_or_shade = parts.next()?;
    if (color_or_shade == "white" || color_or_shade == "black") && parts.next().is_none() {
        return Some(matched_prefix);
    }
    if !COLORS.contains(&color_or_shade) {
        return None;
    }
    let shade = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if shade.len() >= 2 && shade.len() <= 3 && shade.chars().all(|c| c.is_ascii_digit()) {
        Some(matched_prefix)
    } else {
        None
    }
}

fn has_light_counterpart(value: &str, prefix: &str) -> bool {
    value.split_ascii_whitespace().any(|class| {
        let segments: Vec<&str> = class.split(':').collect();
        if segments
            .iter()
            .take(segments.len().saturating_sub(1))
            .any(|s| *s == "dark")
        {
            return false;
        }
        let utility = segments.last().copied().unwrap_or("");
        let utility = utility.trim_start_matches('!').trim_start_matches('-');
        let class_prefix = utility.split('-').next().unwrap_or("");
        class_prefix == prefix && utility.len() > prefix.len()
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dark:"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        if ident.name.as_str() != "className" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let value = lit.value.as_str();

        let paired = value.split_ascii_whitespace().any(|class| {
            dark_raw_color_prefix(class).is_some_and(|prefix| has_light_counterpart(value, prefix))
        });
        if !paired {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual `dark:` color override paired with a light counterpart \u{2014} use a shadcn semantic token (e.g. `bg-background`, `text-foreground`) so theming stays DRY.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn flags_dark_bg_gray_900() {
        assert_eq!(
            run(r#"const x = <div className="bg-white dark:bg-gray-900">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_dark_text_white() {
        assert_eq!(
            run(r#"const x = <div className="text-black dark:text-white">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn allows_dark_semantic_token() {
        assert!(run(r#"const x = <div className="dark:bg-background">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_no_dark_variant() {
        assert!(
            run(r#"const x = <div className="bg-primary text-foreground">x</div>;"#).is_empty()
        );
    }

    #[test]
    fn allows_lone_dark_variant_without_light_counterpart() {
        assert!(run(r#"const x = <div className="dark:bg-gray-900">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_dark_variant_with_unrelated_light_prefix() {
        assert!(
            run(r#"const x = <div className="text-black dark:bg-gray-900">x</div>;"#).is_empty()
        );
    }
}
