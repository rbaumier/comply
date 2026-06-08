//! prefer-set-has OxcCheck backend — flag `const arr = [...]; arr.includes(x)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, VariableDeclarationKind};
use rustc_hash::FxHashSet;
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

        // Phase 1: collect names of `const NAME = [...]` declarations.
        let mut array_names = FxHashSet::default();
        for node in semantic.nodes().iter() {
            if let AstKind::VariableDeclaration(decl) = node.kind() {
                if decl.kind != VariableDeclarationKind::Const {
                    continue;
                }
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init
                        && matches!(init, Expression::ArrayExpression(_))
                            && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                            {
                                array_names.insert(id.name.as_str().to_owned());
                            }
                }
            }
        }

        if array_names.is_empty() {
            return diagnostics;
        }

        // Phase 2: find `.includes(` calls on those names.
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && let Expression::StaticMemberExpression(member) = &call.callee
                    && member.property.name.as_str() == "includes"
                        && let Expression::Identifier(obj) = &member.object
                            && array_names.contains(obj.name.as_str()) {
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
