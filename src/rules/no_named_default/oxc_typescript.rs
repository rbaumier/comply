//! no-named-default oxc backend — flag `import { default as foo }` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        let Some(ref specifiers) = import.specifiers else { return };
        for spec in specifiers {
            let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            let imported_name = named.imported.name().as_str();
            if imported_name != "default" {
                continue;
            }
            let alias = named.local.name.as_str();
            let (line, column) =
                byte_offset_to_line_col(ctx.source, named.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Replace `{{ default as {alias} }}` with `import {alias} from …` \
                     — prefer the default import syntax."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_named_default_import() {
        let d = run_on(r#"import { default as foo } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import foo from"));
    }


    #[test]
    fn flags_named_default_with_others() {
        let d = run_on(r#"import { default as foo, bar } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foo"));
    }


    #[test]
    fn allows_regular_default_import() {
        assert!(run_on(r#"import foo from './m';"#).is_empty());
    }


    #[test]
    fn allows_named_imports() {
        assert!(run_on(r#"import { bar, baz } from './m';"#).is_empty());
    }
}
