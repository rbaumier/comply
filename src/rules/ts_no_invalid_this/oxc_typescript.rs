//! ts-no-invalid-this OXC backend — flag `this` expressions outside
//! classes/object methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_valid_this_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // Walk up from the ThisExpression. The first `this`-binding boundary
    // determines validity:
    // - ArrowFunction: transparent, keep going.
    // - Function inside a MethodDefinition (class method): valid.
    // - Standalone Function: invalid — stop.
    // - Class: valid (property initializer, etc.).
    let mut hit_function = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Class(_) => return true,
            AstKind::ArrowFunctionExpression(_) => continue,
            AstKind::Function(_) => {
                // Mark that we've entered a function scope; need to
                // check if it's wrapped in a MethodDefinition.
                hit_function = true;
            }
            AstKind::MethodDefinition(_) if hit_function => {
                // The Function was a class method — `this` is valid.
                return true;
            }
            AstKind::PropertyDefinition(_) if hit_function => {
                // Property initializer context — valid.
                return true;
            }
            _ => {
                // If we already hit a standalone function (not a method),
                // any other ancestor means `this` is unbound.
                if hit_function {
                    return false;
                }
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ThisExpression(this_expr) = node.kind() else {
                continue;
            };

            if is_valid_this_context(node, semantic) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, this_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`this` used outside a class or valid context — likely a bug."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_this_at_top_level() {
        let diags = run_on("console.log(this);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_class_method() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn flags_this_in_standalone_function() {
        let diags = run_on("function foo() { return this; }");
        assert_eq!(diags.len(), 1);
    }
}
