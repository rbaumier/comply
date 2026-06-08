use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeValue;
use std::sync::Arc;

const RAW_COLORS: &[&str] = &[
    "white", "black", "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber",
    "yellow", "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet",
    "purple", "fuchsia", "pink", "rose",
];

const COLOR_PREFIXES: &[&str] = &["bg-", "text-", "border-", "ring-", "fill-", "stroke-"];

fn is_raw_color_base(base: &str) -> bool {
    for prefix in COLOR_PREFIXES {
        let Some(rest) = base.strip_prefix(prefix) else {
            continue;
        };
        if RAW_COLORS.contains(&rest) {
            return true;
        }
        if let Some((color, shade)) = rest.rsplit_once('-')
            && RAW_COLORS.contains(&color) && shade.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { return };
        let name = name_ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }

        let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value else { return };
        let value = s.value.as_str();

        let has_dark_raw = value.split_whitespace().any(|tok| {
            let Some(rest) = tok.strip_prefix("dark:") else { return false };
            is_raw_color_base(rest)
        });
        if !has_dark_raw {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual `dark:` variant with a raw palette color — use a semantic token (bg-background, text-foreground, …) that already resolves per theme.".into(),
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
    fn flags_dark_bg_raw() {
        assert_eq!(
            run(r#"export const A = () => <div className="bg-white dark:bg-zinc-900" />;"#).len(),
            1
        );
    }


    #[test]
    fn flags_dark_text_raw() {
        assert_eq!(
            run(r#"export const A = () => <div className="dark:text-gray-100" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_semantic_token() {
        assert!(
            run(r#"export const A = () => <div className="bg-background text-foreground" />;"#)
                .is_empty()
        );
    }


    #[test]
    fn allows_dark_on_semantic_token() {
        // `dark:bg-muted` is fine — `muted` is a token, not a raw color.
        assert!(
            run(r#"export const A = () => <div className="bg-card dark:bg-muted" />;"#).is_empty()
        );
    }
}
