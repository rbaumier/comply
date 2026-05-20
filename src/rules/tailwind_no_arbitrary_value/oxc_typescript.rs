//! tailwind-no-arbitrary-value oxc backend.

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
        if !super::has_arbitrary_value(lit.value.as_str()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Arbitrary Tailwind value bypasses design tokens — use the \
                      closest matching token, or define a new token in \
                      `tailwind.config.ts`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_p_arbitrary_px() {
        let src = r#"const x = <div className="p-[16px]" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_token_padding() {
        let src = r#"const x = <div className="p-4" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_variant_brackets() {
        let src = r#"const x = <div className="aria-[expanded=true]:bg-red-500" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_var_composition() {
        let src = r#"const x = <div className="rounded-[var(--radius)]" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_arbitrary_variant_double_bracket() {
        let src = r#"const x = <span className="in-[[data-slot=item][data-checked]]:opacity-100" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_canonical_unit_ch() {
        let src = r#"const x = <p className="max-w-[30ch]" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_function_composition() {
        let src = r#"const x = <div className="bg-[radial-gradient(circle_at_top,oklch(from_var(--color-primary)_calc(l+0.1)_c_h)_0%,transparent_70%)]" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_hex_color() {
        let src = r#"const x = <div className="bg-[#abc]" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_px_value() {
        let src = r#"const x = <div className="w-[42px]" />;"#;
        assert_eq!(run(src).len(), 1);
    }
}
