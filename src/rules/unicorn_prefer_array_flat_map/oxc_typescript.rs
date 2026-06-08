//! unicorn-prefer-array-flat-map oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".flat("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Outer call: `<x>.flat(...)` with no arg (or depth 1).
        let Expression::StaticMemberExpression(outer_member) = &call.callee else {
            return;
        };
        if outer_member.property.name.as_str() != "flat" {
            return;
        }
        // The receiver must be `<x>.map(fn)` — same chain shape.
        let Expression::CallExpression(inner_call) = &outer_member.object else {
            return;
        };
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        if inner_member.property.name.as_str() != "map" {
            return;
        }
        // `.flat(depth)` with depth != 1 isn't equivalent to flatMap.
        // Accept no arg (depth defaults to 1) or `.flat(1)`.
        match call.arguments.first() {
            None => {}
            Some(arg) => {
                let Some(expr) = arg.as_expression() else {
                    return;
                };
                let Expression::NumericLiteral(n) = expr else {
                    return;
                };
                if n.value != 1.0 {
                    return;
                }
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.map(fn).flat()` walks the array twice — use `.flatMap(fn)` \
                      to do it in a single pass."
                .into(),
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
    fn flags_map_flat() {
        let src = "const r = xs.map(x => [x, x + 1]).flat();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_flat_1() {
        let src = "const r = xs.map(x => x).flat(1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_flat_with_deeper_depth() {
        let src = "const r = xs.map(x => x).flat(2);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_standalone_flat() {
        let src = "const r = xs.flat();";
        assert!(run(src).is_empty());
    }
}
