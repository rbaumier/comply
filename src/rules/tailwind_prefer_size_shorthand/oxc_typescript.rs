//! tailwind-prefer-size-shorthand oxc backend for TS / JS / TSX.

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
        if let Some(val) = super::find_wh_duplicate(class_str) {
            let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Replace `w-{val} h-{val}` with `size-{val}`."),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_equal_w_h() {
        assert_eq!(run(r#"const x = <div className="w-4 h-4 flex" />;"#).len(), 1);
    }

    #[test]
    fn flags_full() {
        assert_eq!(run(r#"const x = <div className="w-full h-full" />;"#).len(), 1);
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"const x = <div className="w-4 h-6" />;"#).is_empty());
    }

    #[test]
    fn allows_size_shorthand_already() {
        assert!(run(r#"const x = <div className="size-4" />;"#).is_empty());
    }
}
