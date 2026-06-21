//! OXC backend for node-no-exports-assign.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::{AssignmentTarget, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["exports"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Only flag `exports = ...` (direct assignment to bare identifier `exports`).
        let AssignmentTarget::AssignmentTargetIdentifier(ident) = &assign.left else {
            return;
        };
        if ident.name.as_str() != "exports" {
            return;
        }

        // Only flag the CommonJS module-level `exports` binding. An `exports`
        // that resolves to a local declaration — a function parameter or a
        // `let`/`const`/`var exports` — is an ordinary variable, and assigning
        // to it does not break module exports. A resolved `symbol_id` means a
        // local binding; the CommonJS `exports` global is a free reference with
        // no symbol.
        if let Some(ref_id) = ident.reference_id.get()
            && semantic.scoping().get_reference(ref_id).symbol_id().is_some()
        {
            return;
        }

        // Allow `module.exports = exports = {}` pattern:
        // if parent is also an assignment whose left is `module.exports`, skip.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::AssignmentExpression(parent_assign) = parent.kind()
            && is_module_exports_target(&parent_assign.left) {
                return;
            }

        // Allow `exports = module.exports = {}` pattern.
        if let Expression::AssignmentExpression(ref right) = assign.right
            && is_module_exports_target(&right.left) {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected assignment to `exports` variable. Use `module.exports` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_module_exports_target(target: &AssignmentTarget) -> bool {
    match target {
        AssignmentTarget::StaticMemberExpression(mem) => {
            if mem.property.name.as_str() != "exports" {
                return false;
            }
            matches!(&mem.object, Expression::Identifier(id) if id.name.as_str() == "module")
        }
        _ => false,
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

    #[test]
    fn flags_module_level_exports_assign() {
        let diags = run_on("exports = {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_assignment_to_function_parameter_named_exports() {
        // Regression for #5188: `exports` is a function parameter, not the CJS
        // module global; reassigning it targets the local binding.
        let src = "function _findSubpath(subpath, exports) {\n  if (typeof exports === \"string\") {\n    exports = { \".\": exports };\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_assignment_to_local_variable_named_exports() {
        let src = "function f() {\n  let exports = {};\n  exports = { a: 1 };\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_module_exports_chained_assignment() {
        assert!(run_on("module.exports = exports = {};").is_empty());
    }
}
