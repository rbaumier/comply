//! testing-no-mocktimers-without-restore — OXC backend.
//! Flags `vi.useFakeTimers()` / `jest.useFakeTimers()` calls in files that
//! never pair them with `useRealTimers()` in an afterEach/afterAll.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if a call expression is `vi.useFakeTimers()` or `jest.useFakeTimers()`.
fn is_use_fake_timers(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::Identifier(obj) = &member.object else { return false };
    matches!(obj.name.as_str(), "vi" | "jest") && member.property.name.as_str() == "useFakeTimers"
}

/// Check if a call expression is `vi.useRealTimers()` or `jest.useRealTimers()`.
fn is_use_real_timers(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::Identifier(obj) = &member.object else { return false };
    matches!(obj.name.as_str(), "vi" | "jest") && member.property.name.as_str() == "useRealTimers"
}

/// Check if a node is inside an `afterEach(...)` or `afterAll(...)` callback.
fn is_inside_after_hook(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let Expression::Identifier(id) = &call.callee else { continue };
            if matches!(id.name.as_str(), "afterEach" | "afterAll") {
                return true;
            }
        }
    }
    false
}

/// Does the file contain a `useRealTimers()` call inside an `afterEach` / `afterAll`?
fn has_scoped_real_timer_restore(semantic: &oxc_semantic::Semantic) -> bool {
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else { continue };
        if !is_use_real_timers(call) {
            continue;
        }
        if is_inside_after_hook(node.id(), semantic) {
            return true;
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
        if !ctx.source.contains("useFakeTimers") {
            return Vec::new();
        }
        if has_scoped_real_timer_restore(semantic) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else { continue };
            if !is_use_fake_timers(call) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "useFakeTimers() without a matching useRealTimers() in afterEach/afterAll leaks mocked timers into sibling tests.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_vi_use_fake_timers_without_restore() {
        assert_eq!(run("beforeEach(() => { vi.useFakeTimers(); });").len(), 1);
    }

    #[test]
    fn flags_jest_use_fake_timers_without_restore() {
        assert_eq!(run("beforeAll(() => { jest.useFakeTimers(); });").len(), 1);
    }

    #[test]
    fn allows_pair_with_use_real_timers() {
        let src = "beforeEach(() => { vi.useFakeTimers(); });\n\
                   afterEach(() => { vi.useRealTimers(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_file_without_fake_timers() {
        assert!(run("test('a', () => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn flags_use_real_timers_outside_after_hook() {
        let src = "vi.useRealTimers();\n\
                   beforeEach(() => { vi.useFakeTimers(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_real_timers_in_after_all() {
        let src = "beforeAll(() => { vi.useFakeTimers(); });\n\
                   afterAll(() => { vi.useRealTimers(); });";
        assert!(run(src).is_empty());
    }
}
