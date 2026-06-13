//! jsx-no-undef OXC backend — walk every `JSXOpeningElement` and flag
//! PascalCase tag identifiers that don't resolve to any symbol in the file.

use std::collections::HashSet;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;

pub struct Check;

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if crate::rules::path_utils::is_codemod_fixture_file(ctx.path) {
            return Vec::new();
        }

        let scoping = semantic.scoping();
        let defined: HashSet<String> = scoping.symbol_names().map(|s| s.to_string()).collect();

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXOpeningElement(opening) = node.kind() else { continue };
            let (name, span_start) = match &opening.name {
                JSXElementName::IdentifierReference(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                JSXElementName::Identifier(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                _ => continue,
            };

            if !starts_with_uppercase(name) {
                continue;
            }

            if defined.contains(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{name}` is not defined."),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_undefined_component_in_normal_file() {
        let src = "function App() { return <MyComponent />; }";
        let d = run_on(src, "src/App.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("MyComponent"));
    }

    #[test]
    fn skips_jscodeshift_codemod_fixture_file() {
        let src = r#"
<div>
  <MenuItem
    onClick={() => { analytics('Clicked Menu > Progress'); }}
    primaryText="My Progress"
    rightIcon={<ProgressIcon />}
  />
</div>
"#;
        assert!(
            run_on(src, "packages/mui-codemod/src/v1.0.0/menu-item-primary-text.test/actual.js")
                .is_empty()
        );
        assert!(
            run_on(src, "packages/mui-codemod/src/v1.0.0/menu-item-primary-text.test/expected.js")
                .is_empty()
        );
    }
}
