//! react-jsx-no-duplicate-props oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName};
use rustc_hash::FxHashSet;
use std::sync::Arc;

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let mut seen = FxHashSet::default();
        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(a) = attr else {
                continue;
            };
            let name = match &a.name {
                JSXAttributeName::Identifier(id) => id.name.as_str(),
                JSXAttributeName::NamespacedName(ns) => {
                    // Skip namespaced names — they're rare and not the
                    // typical duplicate-prop pattern.
                    let _ = ns;
                    continue;
                }
            };
            if !seen.insert(name.to_string()) {
                let (line, column) = byte_offset_to_line_col(ctx.source, a.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "react-jsx-no-duplicate-props".into(),
                    message: format!(
                        "Duplicate JSX prop `{name}` \u{2014} the last value silently wins."
                    ),
                    severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_duplicate_prop() {
        let src = r#"const x = <div className="a" className="b" />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_duplicates() {
        let src = r#"const x = <input type="text" value="a" type="number" value="b" />;"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_unique_props() {
        let src = r#"const x = <div className="a" id="b" />;"#;
        assert!(run_on(src).is_empty());
    }
}
