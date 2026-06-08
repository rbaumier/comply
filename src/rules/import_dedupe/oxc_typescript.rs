//! import-dedupe OXC backend — flag duplicate specifiers within one import.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::collections::HashSet;
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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let Some(specifiers) = &import.specifiers else {
            return;
        };

        let mut seen: HashSet<&str> = HashSet::new();
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(s) = spec else {
                continue;
            };
            // Local binding = alias if present (local), else imported name.
            let local_name = s.local.name.as_str();
            if !seen.insert(local_name) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, s.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate specifier `{local_name}` in the same import — remove the redundant entry."
                    ),
                    severity: Severity::Warning,
                    span: Some((s.span.start as usize, (s.span.end - s.span.start) as usize)),
                });
            }
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
    fn flags_duplicate_named_specifier() {
        let d = run_on("import { a, a } from 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate specifier `a`"));
    }


    #[test]
    fn flags_duplicate_alias() {
        let d = run_on("import { a as x, b as x } from 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }


    #[test]
    fn allows_distinct_specifiers() {
        assert!(run_on("import { a, b } from 'x';").is_empty());
    }


    #[test]
    fn allows_alias_with_same_source_name() {
        // `a` and `a as b` bind different locals: `a` and `b`.
        assert!(run_on("import { a, a as b } from 'x';").is_empty());
    }
}
