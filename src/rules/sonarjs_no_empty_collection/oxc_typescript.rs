//! sonarjs-no-empty-collection oxc backend.
//!
//! Conservative scope: only flag iteration of bindings that are *provably*
//! empty forever — i.e. `const X = [] as const` or `const X: readonly [] = []`.
//! Plain `const X = []` is too noisy (often the binding accumulates via
//! external mutation, which static analysis can't always see).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, TSType, TSTypeName, VariableDeclarator,
};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

/// Decide whether a declarator binds a provably-empty-forever array:
/// either `[] as const` or a type-annotation of `readonly []` / `never[]`.
fn is_empty_forever<'a>(decl: &'a VariableDeclarator<'a>) -> bool {
    let Some(init) = &decl.init else { return false };
    // Case A: `[] as const`
    if let Expression::TSAsExpression(as_expr) = init {
        if let Expression::ArrayExpression(arr) = &as_expr.expression
            && arr.elements.is_empty()
            && matches!(
                &as_expr.type_annotation,
                TSType::TSTypeReference(t)
                    if matches!(&t.type_name, TSTypeName::IdentifierReference(id) if id.name.as_str() == "const")
            )
        {
            return true;
        }
    }
    // Case B: explicit `readonly []` / `never[]` type annotation.
    if let Expression::ArrayExpression(arr) = init
        && arr.elements.is_empty()
        && let Some(ann) = &decl.type_annotation
    {
        if let TSType::TSTupleType(tt) = &ann.type_annotation
            && tt.element_types.is_empty()
        {
            return true;
        }
        if let TSType::TSArrayType(at) = &ann.type_annotation
            && let TSType::TSNeverKeyword(_) = &at.element_type
        {
            return true;
        }
    }
    false
}

fn binding_name<'a>(decl: &'a VariableDeclarator<'a>) -> Option<&'a str> {
    match &decl.id {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// Collect every `const X = [] as const` / `readonly []` binding name in
/// the program (top-level + inside functions).
fn collect_empty_bindings<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for node in semantic.nodes().iter() {
        if let AstKind::VariableDeclarator(decl) = node.kind()
            && is_empty_forever(decl)
            && let Some(name) = binding_name(decl)
        {
            names.insert(name.to_string());
        }
    }
    names
}

impl OxcCheck for Check {
    // Stateful: every call site is judged against the file's full set of
    // provably-empty bindings, so the rule runs once per file via
    // `run_on_semantic` (collecting that set once) instead of per-node —
    // a per-node `run` would rebuild the set on every CallExpression /
    // ForOfStatement, i.e. O(nodes²) per file.
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let empties = collect_empty_bindings(semantic);
        if empties.is_empty() {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::CallExpression(call) => {
                    let Expression::StaticMemberExpression(m) = &call.callee else { continue };
                    let Expression::Identifier(obj) = &m.object else { continue };
                    if !empties.contains(obj.name.as_str()) {
                        continue;
                    }
                    let method = m.property.name.as_str();
                    if !matches!(
                        method,
                        "forEach" | "map" | "filter" | "reduce" | "find" | "some" | "every" | "flatMap"
                    ) {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}` is provably empty (declared as `[] as const` / `readonly []`) — \
                             this `.{}()` call is dead code.",
                            obj.name.as_str(),
                            method
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                AstKind::ForOfStatement(stmt) => {
                    let Expression::Identifier(obj) = &stmt.right else { continue };
                    if !empties.contains(obj.name.as_str()) {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}` is provably empty — this `for...of` loop never executes.",
                            obj.name.as_str()
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                _ => {}
            }
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_foreach_on_as_const_empty() {
        let src = "const xs = [] as const; xs.forEach(x => x);";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_for_of_on_as_const_empty() {
        let src = "const xs = [] as const; for (const x of xs) {}";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_plain_empty_array() {
        let src = "const xs: number[] = []; xs.forEach(x => x);";
        assert!(run(src).is_empty());
    }
}
