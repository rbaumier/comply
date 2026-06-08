//! no-instanceof-builtins OXC backend — flag `x instanceof Array` and other builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Built-in constructors the rule keeps flagging.
///
/// `Error` and its subclasses (EvalError, RangeError, …) are *not*
/// listed here. In server-side single-realm Node/Bun apps with no
/// `vm.runInContext`, no `Worker` boundaries and no iframes, the
/// cross-realm concern that motivates avoiding `x instanceof Error`
/// does not apply — and `instanceof Error` is the canonical, well-typed
/// way to narrow an `unknown` thrown value. Forcing every boundary
/// mapper to rewrite the same pattern through a custom helper produces
/// a flood of false positives.
const BUILTINS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "RegExp",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["instanceof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != oxc_ast::ast::BinaryOperator::Instanceof {
            return;
        }

        let Expression::Identifier(id) = &bin.right else { return };
        let name = id.name.as_str();
        if !BUILTINS.contains(&name) {
            return;
        }

        let suggestion = if name == "Array" {
            "Use `Array.isArray(x)` instead.".to_string()
        } else {
            format!("Avoid `instanceof {name}` — it fails across realms.")
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: suggestion,
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_instanceof_array() {
        let src = "const r = x instanceof Array;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_instanceof_map() {
        let src = "const r = x instanceof Map;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_instanceof_error() {
        // Regression for rbaumier/comply#28 — `instanceof Error` is the
        // canonical narrowing for `unknown` thrown values in single-realm
        // Node/Bun. No realistic TS-wide alternative exists.
        let src = r#"
            function fromCaught(value: unknown): Error | null {
                return value instanceof Error ? value : null;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_instanceof_error_subclasses() {
        for cls in ["TypeError", "RangeError", "SyntaxError"] {
            let src = format!("const r = x instanceof {cls};");
            assert!(run(&src).is_empty(), "{cls} should be allowed");
        }
    }
}
