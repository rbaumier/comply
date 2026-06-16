//! generator-without-yield oxc backend — flag generator functions missing `yield`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Whether a computed property key names the iterator protocol, i.e.
/// `[Symbol.iterator]` or `[Symbol.asyncIterator]`.
fn is_iterator_protocol_key(key: &oxc_ast::ast::PropertyKey) -> bool {
    let oxc_ast::ast::PropertyKey::StaticMemberExpression(member) = key else {
        return false;
    };
    matches!(&member.object, oxc_ast::ast::Expression::Identifier(id) if id.name == "Symbol")
        && matches!(member.property.name.as_str(), "iterator" | "asyncIterator")
}

/// Whether this generator is the empty implementation of a `[Symbol.iterator]`
/// / `[Symbol.asyncIterator]` protocol member. An empty generator yields
/// nothing on purpose: it is the idiomatic way to make an object an empty
/// iterable (`for...of` / spread produce an empty sequence), so the absent
/// `yield` is the implementation, not a mistake. A non-empty body that merely
/// forgot to `yield` is still flagged.
fn is_empty_iterator_protocol_generator<'a>(
    func: &oxc_ast::ast::Function<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let empty_body = func.body.as_ref().is_some_and(|b| b.statements.is_empty());
    if !empty_body {
        return false;
    }
    match semantic.nodes().parent_kind(node.id()) {
        AstKind::ObjectProperty(prop) => is_iterator_protocol_key(&prop.key),
        AstKind::MethodDefinition(method) => is_iterator_protocol_key(&method.key),
        _ => false,
    }
}

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

/// Whether the generator's own body, ignoring no-op `EmptyStatement`s, consists
/// solely of a `ThrowStatement`. Such a generator can only throw and never
/// yields on purpose: it is the idiomatic way to build a failing
/// `AsyncIterable<never>`, so the absent `yield` is the implementation. String
/// directive prologues (e.g. `"use strict"`) live in `body.directives`, not in
/// `statements`, so they do not affect this check.
fn is_throw_only_generator(func: &oxc_ast::ast::Function) -> bool {
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    let mut executable = body
        .statements
        .iter()
        .filter(|stmt| !matches!(stmt, oxc_ast::ast::Statement::EmptyStatement(_)));
    matches!(executable.next(), Some(oxc_ast::ast::Statement::ThrowStatement(_)))
        && executable.next().is_none()
}

/// Whether this generator performs an `await` inside a loop that belongs to its
/// own async work. An async generator awaiting in a loop (commonly until an
/// abort signal) is a deliberate long-running / wait pattern that defers any
/// `yield` to the loop, so the absent `yield` is not a mistake.
///
/// An `AwaitExpression` counts only when, walking up from it to the nearest
/// function ancestor, that ancestor is this generator (the await is THIS
/// generator's work, not an inner closure's) and a loop statement is crossed on
/// the way (the await runs inside the loop, not merely beside it).
fn has_await_in_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let node_id = node.id();
    for snode in semantic.nodes().iter() {
        if !matches!(snode.kind(), AstKind::AwaitExpression(_)) {
            continue;
        }
        let mut cur = snode.id();
        let mut crossed_loop = false;
        loop {
            let parent_id = semantic.nodes().parent_id(cur);
            if parent_id == cur {
                break;
            }
            if parent_id == node_id {
                if crossed_loop {
                    return true;
                }
                break;
            }
            let parent = semantic.nodes().get_node(parent_id);
            match parent.kind() {
                // Stop at nested function boundaries: this await is an inner
                // closure's work, not this generator's.
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => break,
                AstKind::WhileStatement(_)
                | AstKind::DoWhileStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::ForOfStatement(_) => crossed_loop = true,
                _ => {}
            }
            cur = parent_id;
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
        if is_empty_iterator_protocol_generator(func, node, semantic) {
            return;
        }
        // A throw-only generator is an intentional failing `AsyncIterable<never>`.
        if is_throw_only_generator(func) {
            return;
        }
        // An await inside a loop is a deliberate long-running / wait pattern.
        if has_await_in_loop(node, semantic) {
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

    // Regression for issue #3362: an empty generator assigned to `[Symbol.iterator]`
    // is the idiomatic empty-iterable implementation, not a missing-yield mistake.
    #[test]
    fn allows_empty_symbol_iterator_property_generator() {
        let src = "\
module.exports = function noopSet () {
  return {
    [Symbol.iterator]: function * () {},
    add () {},
    delete () {},
    has () { return true }
  }
}
";
        assert!(
            run_at(src, "lib/noop-set.js").is_empty(),
            "empty [Symbol.iterator] generator is an intentional empty iterable"
        );
    }

    #[test]
    fn allows_empty_symbol_iterator_method_shorthand() {
        let src = "const o = {\n  *[Symbol.iterator]() {}\n}";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    #[test]
    fn allows_empty_symbol_async_iterator_class_method() {
        let src = "class C {\n  async *[Symbol.asyncIterator]() {}\n}";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    // Over-exemption guard: a `[Symbol.iterator]` generator with a non-empty body
    // that merely forgot to `yield` is still a real bug and must flag.
    #[test]
    fn flags_nonempty_symbol_iterator_generator_without_yield() {
        let src = "\
const o = {
  *[Symbol.iterator]() {
    return 42;
  }
}
";
        assert_eq!(run_at(src, "src/index.ts").len(), 1);
    }

    // Regression for issue #3319, Pattern A: a generator whose body only throws
    // is an intentional failing `AsyncIterable<never>`.
    #[test]
    fn allows_throw_only_async_generator() {
        let src = "async function* () {\n  throw err;\n}";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    // Regression for issue #3319, Pattern A: the tRPC production case —
    // a throw-only generator wrapping an error into an AsyncIterable.
    #[test]
    fn allows_throw_only_generator_in_run_call() {
        let src = "run(async function* () {\n  throw new TRPCError('x');\n});";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    // Regression for issue #3319, Pattern B: an await inside a `while` loop that
    // runs until an abort signal is a deliberate long-running subscription.
    #[test]
    fn allows_await_in_while_loop_generator() {
        let src = "async function* (opts) {\n  while (!opts.signal.aborted) {\n    await sleep(10);\n  }\n}";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    // Regression for issue #3319, Pattern B: a `for await` loop with an await in
    // its body is also a deliberate wait pattern.
    #[test]
    fn allows_for_await_loop_generator() {
        let src = "async function* () {\n  for await (const x of src) {\n    await handle(x);\n  }\n}";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    // Over-exemption guard (issue #3319): a generator that returns instead of
    // yielding — no throw, no loop — is the classic forgot-to-yield bug.
    #[test]
    fn flags_generator_that_returns_array() {
        let src = "function* range() {\n  return [1, 2, 3];\n}";
        assert_eq!(run_at(src, "src/index.ts").len(), 1);
    }

    // Over-exemption guard (issue #3319): a straight-line body with no yield,
    // throw, or loop is still flagged.
    #[test]
    fn flags_straight_line_generator() {
        let src = "function* foo() {\n  const a = compute();\n  doStuff(a);\n}";
        assert_eq!(run_at(src, "src/index.ts").len(), 1);
    }

    // Over-exemption guard (issue #3319): a single await NOT inside a loop is
    // still flagged — Pattern B requires the await to be inside a loop.
    #[test]
    fn flags_async_generator_with_await_not_in_loop() {
        let src = "async function* bar() {\n  await setup();\n}";
        assert_eq!(run_at(src, "src/index.ts").len(), 1);
    }
}
