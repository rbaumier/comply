//! tailwind-no-duplicate-classes oxc backend for TS / JS / TSX.

use rustc_hash::FxHashSet;

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
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        for class in class_str.split_whitespace() {
            if !seen.insert(class) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Duplicate class `{class}` — remove the repetition."),
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
    fn flags_duplicate_classname() {
        let diags = run(r#"const x = <div className="p-4 mt-2 p-4" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-4"));
    }

    #[test]
    fn flags_duplicate_class_attr() {
        let diags = run(r#"const x = <div class="text-lg text-lg" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-lg"));
    }

    #[test]
    fn allows_unique_classes() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let diags = run(r#"const x = <div className="p-4 mt-2 p-4 mt-2" />;"#);
        assert_eq!(diags.len(), 2);
    }
}
