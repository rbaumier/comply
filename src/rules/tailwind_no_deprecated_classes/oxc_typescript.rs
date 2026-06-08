//! tailwind-no-deprecated-classes oxc backend for TS / JS / TSX.

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
        for class in class_str.split_whitespace() {
            let base = class.rsplit(':').next().unwrap_or(class);
            let base = base.strip_prefix('!').unwrap_or(base);
            if let Some(replacement) = super::replacement_for(base) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Deprecated Tailwind class `{base}` — use `{replacement}` instead."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_flex_grow_0() {
        let diags = run(r#"const x = <div className="flex-grow-0" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grow-0"));
    }

    #[test]
    fn flags_overflow_ellipsis() {
        let diags = run(r#"const x = <div className="truncate overflow-ellipsis" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-ellipsis"));
    }

    #[test]
    fn allows_current_classes() {
        assert!(run(r#"const x = <div className="grow shrink p-4 text-ellipsis" />;"#).is_empty());
    }

    #[test]
    fn flags_with_variant() {
        let diags = run(r#"const x = <div className="hover:flex-shrink" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink"));
    }

    #[test]
    fn allows_overflow_clip() {
        // overflow-clip is a valid Tailwind utility (overflow: clip), not deprecated.
        // It is distinct from text-clip (text-overflow: clip).
        assert!(run(r#"const x = <div className="overflow-clip" />;"#).is_empty());
    }
}
