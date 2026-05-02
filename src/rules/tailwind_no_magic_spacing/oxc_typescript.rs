//! tailwind-no-magic-spacing oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "class"])
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
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let class_str = lit.value.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        for token in class_str.split_whitespace() {
            for prefix in super::SPACING_PREFIXES {
                if let Some(rest) = token.strip_prefix(prefix)
                    && let Some(value) = rest.strip_suffix(']')
                    && let Some(n) = super::parse_px(value)
                    && n % 4 != 0
                {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}{value}]` uses {n}px which is not a multiple of 4 — stick to the design-token spacing scale.",
                            prefix.trim_end_matches('[')
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_non_multiple_of_four_padding() {
        assert_eq!(run(r#"const x = <div className="p-[13px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_margin_seven() {
        assert_eq!(run(r#"const x = <div className="m-[7px]" />;"#).len(), 1);
    }

    #[test]
    fn allows_multiple_of_four() {
        assert!(run(r#"const x = <div className="p-[16px]" />;"#).is_empty());
    }

    #[test]
    fn allows_rem_unit() {
        assert!(run(r#"const x = <div className="p-[1.5rem]" />;"#).is_empty());
    }

    #[test]
    fn allows_standard_scale() {
        assert!(run(r#"const x = <div className="p-4" />;"#).is_empty());
    }
}
