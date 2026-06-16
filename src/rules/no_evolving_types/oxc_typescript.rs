//! no-evolving-types OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, VariableDeclarator};
use std::sync::Arc;

pub struct Check;

/// True when this declarator's shape would let TypeScript infer an evolving
/// implicit type. Three cases, all requiring no type annotation:
/// - no initializer at all (`let a;`)
/// - initialized to `null` (`let c = null;`)
/// - initialized to an empty array (`const b = [];`)
fn evolves_to_any(declarator: &VariableDeclarator) -> bool {
    if declarator.type_annotation.is_some() {
        return false;
    }
    match &declarator.init {
        None => true,
        Some(Expression::NullLiteral(_)) => true,
        Some(Expression::ArrayExpression(array)) => array.elements.is_empty(),
        Some(_) => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else { return };

        // `for (const x of xs)` / `for (const k in obj)` bindings are bound by
        // the loop on every iteration — their implicit type cannot evolve, so
        // Biome's `JsVariableDeclaration` query never reaches them. C-style
        // `for (let a = 0, b; ...)` heads stay in scope, so they are flagged.
        let parent = semantic.nodes().parent_kind(node.id());
        if matches!(parent, AstKind::ForOfStatement(_) | AstKind::ForInStatement(_)) {
            return;
        }

        for declarator in &decl.declarations {
            if !evolves_to_any(declarator) {
                continue;
            }
            // Only simple identifier bindings carry an evolving type. Biome's
            // diagnostic bails on destructuring patterns.
            let BindingPattern::BindingIdentifier(id) = &declarator.id else { continue };

            let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "The type of this variable may evolve implicitly to the `any` type. \
                          Add an explicit type or initialization to avoid implicit type evolution."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // ---- Invalid fixtures (Biome invalid.ts) — each must be flagged ----

    #[test]
    fn flags_uninitialized_let() {
        assert_eq!(run_on("let a;").len(), 1);
    }

    #[test]
    fn flags_empty_array_const() {
        assert_eq!(run_on("const b = [];").len(), 1);
    }

    #[test]
    fn flags_null_initialized_let() {
        assert_eq!(run_on("let c = null;").len(), 1);
    }

    #[test]
    fn flags_uninitialized_var() {
        assert_eq!(run_on("var someVar1;").len(), 1);
    }

    #[test]
    fn flags_only_uninitialized_declarator_in_list() {
        // `let x = 0, y, z = 0;` — only `y` evolves.
        assert_eq!(run_on("let x = 0, y, z = 0;").len(), 1);
        assert_eq!(run_on("var x = 0, y, z = 0;").len(), 1);
    }

    #[test]
    fn flags_uninitialized_in_c_style_for_head() {
        // `for(let a = 0, b; a < 5; a++) {}` — `b` evolves.
        assert_eq!(run_on("for (let a = 0, b; a < 5; a++) {}").len(), 1);
    }

    #[test]
    fn flags_uninitialized_in_function_body() {
        assert_eq!(run_on("function ex() { let b; }").len(), 1);
    }

    // ---- Valid fixtures (Biome valid.ts) — none may be flagged ----

    #[test]
    fn allows_annotated_without_init() {
        assert!(run_on("let a: number;").is_empty());
        assert!(run_on("var c : string;").is_empty());
    }

    #[test]
    fn allows_initialized_literal() {
        assert!(run_on("let b = 1;").is_empty());
        assert!(run_on("var d = \"abn\";").is_empty());
        assert!(run_on("const x = 0;").is_empty());
    }

    #[test]
    fn allows_annotated_empty_array() {
        assert!(run_on("const e: never[] = [];").is_empty());
    }

    #[test]
    fn allows_non_empty_array() {
        assert!(run_on("const f = [null];").is_empty());
        assert!(run_on("const g = ['1'];").is_empty());
        assert!(run_on("const h = [1];").is_empty());
    }

    #[test]
    fn allows_annotated_null() {
        assert!(run_on("let workspace: Workspace | null = null;").is_empty());
    }

    #[test]
    fn allows_for_of_binding() {
        // `for(let y of xs) {}` — loop-bound, not evolving.
        assert!(run_on("for (let y of xs) {}").is_empty());
    }

    #[test]
    fn allows_for_in_binding() {
        assert!(run_on("for (let k in obj) {}").is_empty());
    }

    #[test]
    fn allows_using_declaration() {
        // `using z = f();` — initialized.
        assert!(run_on("using z = f();").is_empty());
    }

    #[test]
    fn ignores_destructuring_patterns() {
        // Destructuring has no single evolving identifier; Biome's diagnostic
        // bails on these.
        assert!(run_on("let { a } = obj;").is_empty());
        assert!(run_on("let [a] = arr;").is_empty());
    }
}
