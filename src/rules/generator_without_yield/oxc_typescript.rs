//! generator-without-yield oxc backend — flag generator functions missing `yield`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Walk semantic descendants of a node to check if any is a YieldExpression,
/// but stop at nested function boundaries (they have their own generator scope).
fn has_yield_in_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let node_id = node.id();
    for snode in semantic.nodes().iter() {
        if let AstKind::YieldExpression(_) = snode.kind() {
            // Check if this yield's nearest function ancestor is our node.
            let mut cur = snode.id();
            loop {
                let parent_id = semantic.nodes().parent_id(cur);
                if parent_id == cur {
                    break;
                }
                if parent_id == node_id {
                    return true;
                }
                let parent = semantic.nodes().get_node(parent_id);
                // Stop at nested function boundaries.
                match parent.kind() {
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => break,
                    _ => {}
                }
                cur = parent_id;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };
        if !func.generator {
            return;
        }
        // `*.test-d.{ts,tsx}` are tsd / `expect-type` type-declaration tests:
        // an empty `function* () {}` there asserts the inferred generator type
        // shape (a resolver that yields nothing), checked by `tsc --noEmit` and
        // never executed, so a missing `yield` is the contract under test.
        if crate::rules::path_utils::has_test_d_infix(ctx.path) {
            return;
        }
        if has_yield_in_body(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, func.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Generator function does not contain a `yield` — add one or use a regular function."
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_empty_generator_in_regular_file() {
        let d = run_at("function* gen() {\n  return 42;\n}", "src/index.ts");
        assert_eq!(d.len(), 1);
    }

    // Regression for issue #1827: empty generators in `*.test-d.{ts,tsx}` type
    // tests assert the inferred generator type shape (msw resolvers that yield
    // nothing); they are checked by `tsc --noEmit` and never executed.
    #[test]
    fn allows_empty_generator_in_test_d_ts() {
        let src = "\
import { http } from 'msw'

it('supports returning nothing from generator resolvers', () => {
  http.get<never, never, { value: string }>('/', function* () {})
  http.get<never, never, { value: string }>('/', async function* () {})
})

it('supports returning undefined from generator resolvers', () => {
  http.get<never, never, { value: string }>('/', function* () {
    return undefined
  })
})
";
        assert!(
            run_at(src, "test/typings/resolver-generator.test-d.ts").is_empty(),
            "empty generator in a .test-d.ts type-declaration test must not be flagged"
        );
    }

    #[test]
    fn allows_empty_generator_in_test_d_tsx() {
        assert!(run_at("function* gen() {}", "src/Component.test-d.tsx").is_empty());
    }
}
