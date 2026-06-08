//! ts-no-redeclare backend — detect duplicate variable declarations in
//! the same scope, via oxc_semantic.
//!
//! Walks every symbol and reports each entry returned by
//! `symbol_redeclarations` (oxc tracks duplicate `var`, `function` and
//! TS declaration-merging compatible redeclarations on the original
//! symbol).
//!
//! The previous implementation rebuilt scope identity from
//! tree-sitter parent kinds, which only supported `function` /
//! `function_declaration` / `arrow_function` / `method_definition` /
//! `statement_block` and treated for-headers, catch clauses, switch
//! blocks, and class bodies as the surrounding scope — leading to both
//! false positives (declarations in for-init reused in the loop body)
//! and false negatives (`for (let x of …)` followed by another `let x`
//! in the same block was missed when the for-statement was nested).
//! Using oxc's symbol model removes that whole class of bugs.

use oxc_ast::AstKind;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let nodes = semantic.nodes();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let decl_ids: Vec<_> = scoping.symbol_declarations(symbol_id).collect();
                if decl_ids.len() <= 1 {
                    continue;
                }

                let all_functions = decl_ids.iter().all(|&id| {
                    matches!(nodes.kind(id), AstKind::Function(_))
                });
                if all_functions {
                    continue;
                }

                let name = scoping.symbol_name(symbol_id);
                for &decl_id in &decl_ids[1..] {
                    let span = nodes.kind(decl_id).span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "ts-no-redeclare".into(),
                        message: format!("`{name}` is already defined."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }

            diagnostics
        })
    }
}

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_duplicate_var() {
        let d = run_on("var x = 1; var x = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_scopes() {
        let d = run_on("function a() { let x = 1; } function b() { let x = 2; }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_function_overloads() {
        let d = run_on("function foo(a: string): string;\nfunction foo(a: number): number;\nfunction foo(a: any): any { return a; }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_duplicate_function_declarations() {
        // Two function declarations = valid TS overload pattern
        let d = run_on("function foo() {} function foo() {}");
        assert!(d.is_empty());
    }
}
