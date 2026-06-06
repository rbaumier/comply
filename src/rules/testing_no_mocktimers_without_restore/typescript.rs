//! testing-no-mocktimers-without-restore backend — flag
//! `vi.useFakeTimers()` / `jest.useFakeTimers()` in a file that never
//! calls the corresponding `useRealTimers()`.
//!
//! Why: fake timers installed in one test leak into the next test if not
//! restored, producing bizarre time-related flakes. Always pair the fake
//! timer install with a real-timer restore in `afterEach` (or `afterAll`).
//!
//! Implementation: cheap file-level pre-filter using `ctx.source`, then
//! emit diagnostics at the exact `useFakeTimers()` call sites.

use crate::diagnostic::{Diagnostic, Severity};

/// Is `func` a `vi.useFakeTimers` / `jest.useFakeTimers` member expression?
fn is_use_fake_timers(func: tree_sitter::Node, source: &[u8]) -> bool {
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let obj_txt = obj.utf8_text(source).unwrap_or("");
    let prop_txt = prop.utf8_text(source).unwrap_or("");
    matches!(obj_txt, "vi" | "jest") && prop_txt == "useFakeTimers"
}

/// Walk ancestors looking for an `afterEach(...)` / `afterAll(...)` call —
/// the only legitimate hosts for a `useRealTimers()` restore.
fn is_inside_after_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(name) = func.utf8_text(source)
            && matches!(name, "afterEach" | "afterAll")
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Does the file contain a `useRealTimers()` call inside an `afterEach` /
/// `afterAll` callback? File-level cache so the per-node walk stays cheap.
fn has_scoped_real_timer_restore(tree: &tree_sitter::Tree, source: &[u8]) -> bool {
    let mut found = false;
    crate::rules::walker::walk_tree(tree, |n| {
        if found {
            return;
        }
        if n.kind() != "call_expression" {
            return;
        }
        let Some(func) = n.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "member_expression" {
            return;
        }
        let Some(prop) = func.child_by_field_name("property") else {
            return;
        };
        if prop.utf8_text(source).unwrap_or("") != "useRealTimers" {
            return;
        }
        if is_inside_after_hook(n, source) {
            found = true;
        }
    });
    found
}

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(
        &self,
        ctx: &crate::rules::backend::CheckCtx,
        tree: &tree_sitter::Tree,
    ) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        if !ctx.source_contains("useFakeTimers") {
            return Vec::new();
        }
        if has_scoped_real_timer_restore(tree, source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        crate::rules::walker::walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(func) = node.child_by_field_name("function") else {
                return;
            };
            if !is_use_fake_timers(func, source) {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "useFakeTimers() without a matching useRealTimers() in afterEach/afterAll leaks mocked timers into sibling tests.".into(),
                Severity::Warning,
            ));
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
        // useRealTimers exists but lives at the top level, not inside
        // afterEach/afterAll → still leaks across tests.
        let src = "vi.useRealTimers();\n\
                   beforeEach(() => { vi.useFakeTimers(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_real_timers_inside_test_body() {
        // Restore inside a test body doesn't reset between tests.
        let src = "beforeEach(() => { vi.useFakeTimers(); });\n\
                   test('a', () => { vi.useRealTimers(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_real_timers_in_after_all() {
        let src = "beforeAll(() => { vi.useFakeTimers(); });\n\
                   afterAll(() => { vi.useRealTimers(); });";
        assert!(run(src).is_empty());
    }
}
