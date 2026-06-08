//! tailwind-no-conflicting-classes oxc backend for TS / JS / TSX.

use std::collections::HashMap;

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
        let classes: Vec<&str> = class_str.split_whitespace().collect();
        let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
        for class in &classes {
            if let Some(key) = super::conflict_key(class) {
                groups.entry(key).or_default().push(class);
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        for (prefix, members) in &groups {
            if members.len() >= 2 {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Conflicting `{prefix}` classes: {} — keep only one.",
                        members.join(", "),
                    ),
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
    fn flags_conflicting_padding() {
        let diags = run(r#"const x = <div className="p-4 p-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-"));
    }

    #[test]
    fn flags_conflicting_text_size() {
        let diags = run(r#"const x = <div className="text-sm text-lg" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_display_conflict() {
        let diags = run(r#"const x = <div className="flex hidden" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("display"));
    }

    #[test]
    fn allows_non_conflicting() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }

    #[test]
    fn allows_text_size_with_text_wrap() {
        assert!(run(r#"const x = <div className="text-2xl text-balance" />;"#).is_empty());
    }

    #[test]
    fn allows_flex_shorthand_with_flex_direction() {
        assert!(run(r#"const x = <div className="flex-1 flex-col" />;"#).is_empty());
    }
}
