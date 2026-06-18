//! prefer-set-has OxcCheck backend — flag `const arr = [...]; arr.includes(x)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, VariableDeclarationKind};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".includes"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        use crate::rules::backend::AstKind;
        let mut diagnostics = Vec::new();

        // Phase 1: collect `const NAME = [...]` declarations with their element
        // count — the count gates emission in phase 2.
        let mut array_lens: FxHashMap<&str, usize> = FxHashMap::default();
        for node in semantic.nodes().iter() {
            if let AstKind::VariableDeclaration(decl) = node.kind() {
                if decl.kind != VariableDeclarationKind::Const {
                    continue;
                }
                for declarator in &decl.declarations {
                    if let Some(Expression::ArrayExpression(array)) = &declarator.init
                        && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                            &declarator.id
                    {
                        array_lens.insert(id.name.as_str(), array.elements.len());
                    }
                }
            }
        }

        if array_lens.is_empty() {
            return diagnostics;
        }

        // Below this element count a linear scan beats a `Set`, so suggesting
        // `Set#has()` would be a pessimization. Authoritative in defaults.toml.
        let min_array_len = ctx.config.threshold("prefer-set-has", "min_array_len", ctx.lang);

        // Phase 2: find `.includes(` calls on those names.
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && let Expression::StaticMemberExpression(member) = &call.callee
                    && member.property.name.as_str() == "includes"
                        && let Expression::Identifier(obj) = &member.object
                            && array_lens.get(obj.name.as_str()).is_some_and(|&len| len >= min_array_len) {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: format!(
                                        "`{}` is a const array used with `.includes()` — consider using a `Set` with `.has()` for O(1) lookups.",
                                        obj.name.as_str()
                                    ),
                                    severity: Severity::Warning,
                                    span: None,
                                });
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Regression for #3967: a 2-element const array is too small for a `Set`
    // to pay off — a linear scan beats hashing + allocation.
    #[test]
    fn allows_two_element_array() {
        let src = "const A = ['class', 'style']; A.includes(name);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_three_element_array() {
        let src = "const A = ['a', 'b', 'c']; A.includes(x);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // The O(1) win is real at/above the threshold (4) — must still flag.
    #[test]
    fn flags_four_element_array() {
        let src = "const A = ['a', 'b', 'c', 'd']; A.includes(x);";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("A") && d[0].message.contains("Set"));
    }
}
