//! testing-no-shared-signal OXC backend: signal declarators and their reads.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, iterating_callback_element_index};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, CallExpression, Expression};
use oxc_semantic::{AstNode, AstNodes, NodeId, Semantic};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Signal {
    Timeout,
    Controller,
}

/// Reports timeout signals and controllers shared across tests or requests.
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["AbortSignal", "AbortController"])
    }

    fn run_on_semantic<'a>(&self, semantic: &'a Semantic<'a>, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let is_test_file = ctx.file.path_segments.in_test_dir;
        semantic
            .nodes()
            .iter()
            .filter_map(|node| shared_signal_diagnostic(semantic, node, ctx, is_test_file))
            .collect()
    }
}

fn shared_signal_diagnostic(
    semantic: &Semantic,
    node: &AstNode,
    ctx: &CheckCtx,
    is_test_file: bool,
) -> Option<Diagnostic> {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
        return None;
    };
    let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
        return None;
    };
    let kind = declarator.init.as_ref().and_then(signal_kind)?;
    let symbol_id = ident.symbol_id.get()?;
    let nodes = semantic.nodes();
    if !is_shared_scope(nodes, node.id(), is_test_file) {
        return None;
    }
    let mut reads = semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .map(|r| r.node_id());
    let is_shared = match (is_test_file, kind) {
        (true, Signal::Timeout) => true,
        (true, Signal::Controller) => {
            let tests: FxHashSet<u32> = reads
                .filter_map(|read| enclosing_test_call(nodes, read))
                .collect();
            tests.len() >= 2
        }
        (false, Signal::Timeout) => reads.any(|read| is_read_per_call(nodes, read)),
        (false, Signal::Controller) => false,
    };
    if !is_shared {
        return None;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
    let name = ident.name.as_str();
    let message = match kind {
        Signal::Timeout => format!(
            "'{name}' is an AbortSignal timeout created once: its budget runs from creation, so a later test or request inherits a spent or aborted signal. Create it where the request is made."
        ),
        Signal::Controller => format!(
            "'{name}' is one AbortController shared by several tests: one abort reaches every later test. Create it in the test or in beforeEach()."
        ),
    };
    Some(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Error,
        span: None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestBlock {
    Test,
    Describe,
}

/// Hooks, `test.step` and other members are neither kind.
fn test_block(callee: &Expression) -> Option<TestBlock> {
    match callee {
        Expression::Identifier(id) => match id.name.as_str() {
            "describe" | "suite" => Some(TestBlock::Describe),
            "test" | "it" => Some(TestBlock::Test),
            _ => None,
        },
        Expression::CallExpression(call) => test_block(&call.callee),
        Expression::StaticMemberExpression(member) => {
            let object = test_block(&member.object)?;
            match member.property.name.as_str() {
                "describe" => Some(TestBlock::Describe),
                "only" | "skip" | "each" | "concurrent" | "fixme" | "fail" | "todo"
                | "parallel" | "serial" => Some(object),
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_timeout_call(expr: &Expression) -> bool {
    matches!(expr, Expression::CallExpression(call)
        if call.callee.is_specific_member_access("AbortSignal", "timeout"))
}

fn signal_kind(init: &Expression) -> Option<Signal> {
    match init.get_inner_expression() {
        expr if is_timeout_call(expr) => Some(Signal::Timeout),
        Expression::CallExpression(call)
            if call.callee.is_specific_member_access("AbortSignal", "any") =>
        {
            let Some(Argument::ArrayExpression(array)) = call.arguments.first() else {
                return None;
            };
            array
                .elements
                .iter()
                .any(|element| element.as_expression().is_some_and(is_timeout_call))
                .then_some(Signal::Timeout)
        }
        Expression::NewExpression(new) if new.callee.is_specific_id("AbortController") => {
            Some(Signal::Controller)
        }
        _ => None,
    }
}

/// Module scope, or a describe body in test files.
fn is_shared_scope(nodes: &AstNodes, declarator_id: NodeId, is_test_file: bool) -> bool {
    let mut ancestors = nodes.ancestor_ids(declarator_id).skip(1);
    match ancestors.next().map(|id| nodes.kind(id)) {
        Some(AstKind::Program(_) | AstKind::ExportNamedDeclaration(_)) => true,
        Some(AstKind::FunctionBody(_)) if is_test_file => ancestors
            .next()
            .and_then(|id| call_receiving(nodes, id))
            .is_some_and(|call| test_block(&call.callee) == Some(TestBlock::Describe)),
        _ => false,
    }
}

/// The call that receives `function_id` as an argument.
fn call_receiving<'a>(nodes: &AstNodes<'a>, function_id: NodeId) -> Option<&'a CallExpression<'a>> {
    let function_start = nodes.kind(function_id).span().start;
    let parent = nodes
        .ancestor_ids(function_id)
        .find(|id| !matches!(nodes.kind(*id), AstKind::ParenthesizedExpression(_)))?;
    match nodes.kind(parent) {
        AstKind::CallExpression(call) if function_start >= call.callee.span().end => Some(call),
        _ => None,
    }
}

fn is_function(nodes: &AstNodes, id: NodeId) -> bool {
    matches!(
        nodes.kind(id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

/// Innermost `test`/`it` call around `read`, by start offset.
fn enclosing_test_call(nodes: &AstNodes, read: NodeId) -> Option<u32> {
    nodes
        .ancestor_ids(read)
        .filter(|id| is_function(nodes, *id))
        .find_map(|id| {
            let call = call_receiving(nodes, id)?;
            (test_block(&call.callee) == Some(TestBlock::Test)).then_some(call.span.start)
        })
}

/// `read` runs per call: exported function or passed callback.
/// Array-iteration callbacks run within the caller's single pass.
fn is_read_per_call(nodes: &AstNodes, read: NodeId) -> bool {
    let Some(outermost) = nodes
        .ancestor_ids(read)
        .filter(|id| is_function(nodes, *id))
        .last()
    else {
        return false;
    };
    if let Some(call) = call_receiving(nodes, outermost) {
        return !matches!(&call.callee, Expression::StaticMemberExpression(member)
            if iterating_callback_element_index(member.property.name.as_str()).is_some());
    }
    let mut ancestors = nodes.ancestor_ids(outermost).map(|id| nodes.kind(id));
    match ancestors.next() {
        Some(AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)) => true,
        Some(AstKind::VariableDeclarator(_)) => matches!(
            (ancestors.next(), ancestors.next()),
            (
                Some(AstKind::VariableDeclaration(_)),
                Some(AstKind::ExportNamedDeclaration(_))
            )
        ),
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
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn flags_module_timeout_read_in_test() {
        let src = r#"const signal = AbortSignal.timeout(1000);
it("x", () => fetch(u, { signal }));
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/a.test.ts").len(), 1);
    }

    #[test]
    fn flags_any_wrapped_timeout_at_describe_scope() {
        let src = r#"describe("d", () => {
  const s = AbortSignal.any([AbortSignal.timeout(1000), other]);
  it("x", () => fetch(u, { signal: s }));
});
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/a.test.ts").len(), 1);
    }

    #[test]
    fn flags_controller_read_by_two_tests() {
        let src = r#"const ctrl = new AbortController();
it("a", () => fetch(u, { signal: ctrl.signal }));
it("b", () => { ctrl.abort(); });
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/a.test.ts").len(), 1);
    }

    #[test]
    fn ignores_controller_read_by_one_test() {
        let src = r#"const ctrl = new AbortController();
it("a", () => fetch(u, { signal: ctrl.signal }));
it("b", () => fetch(u));
"#;
        assert!(run_rule_gated(&Check, src, "src/a.test.ts").is_empty());
    }

    #[test]
    fn ignores_controller_read_by_one_test_with_steps_and_hook() {
        let src = r#"const ctrl = new AbortController();
test("a", async () => {
  await test.step("s1", () => fetch(u, { signal: ctrl.signal }));
  await test.step("s2", () => fetch(v, { signal: ctrl.signal }));
});
test.afterEach(() => ctrl.abort());
"#;
        assert!(run_rule_gated(&Check, src, "src/a.test.ts").is_empty());
    }

    #[test]
    fn flags_timeout_at_playwright_describe_scope() {
        let src = r#"test.describe("d", () => {
  const s = AbortSignal.timeout(1000);
  test("x", () => fetch(u, { signal: s }));
});
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/a.test.ts").len(), 1);
    }

    #[test]
    fn ignores_timeout_read_by_top_level_array_iteration() {
        let src = r#"const signal = AbortSignal.timeout(1000);
await Promise.all(urls.map((u) => fetch(u, { signal })));
"#;
        assert!(run_rule_gated(&Check, src, "src/cli.ts").is_empty());
    }

    #[test]
    fn ignores_timeout_created_in_test() {
        let src = r#"it("x", () => {
  const s = AbortSignal.timeout(1000);
  return fetch(u, { signal: s });
});
"#;
        assert!(run_rule_gated(&Check, src, "src/a.test.ts").is_empty());
    }

    #[test]
    fn flags_timeout_read_in_exported_handler() {
        let src = r#"const signal = AbortSignal.timeout(1000);
export function handler() { return fetch(u, { signal }); }
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/server.ts").len(), 1);
    }

    #[test]
    fn flags_timeout_read_in_route_callback() {
        let src = r#"const signal = AbortSignal.timeout(1000);
app.get("/", () => fetch(u, { signal }));
"#;
        assert_eq!(run_rule_gated(&Check, src, "src/server.ts").len(), 1);
    }

    #[test]
    fn ignores_timeout_read_by_local_main() {
        let src = r#"const signal = AbortSignal.timeout(1000);
async function main() { await fetch(u, { signal }); }
main();
"#;
        assert!(run_rule_gated(&Check, src, "src/cli.ts").is_empty());
    }

    #[test]
    fn ignores_factory_and_default_param() {
        let src = r#"const make = () => AbortSignal.timeout(1000);
function f(signal = AbortSignal.timeout(1000)) { return fetch(u, { signal }); }
export function handler() { return fetch(u, { signal: make() }); }
"#;
        assert!(run_rule_gated(&Check, src, "src/a.test.ts").is_empty());
        assert!(run_rule_gated(&Check, src, "src/server.ts").is_empty());
    }
}
