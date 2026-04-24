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
    if func.kind() != "member_expression" { return false; }
    let Some(obj) = func.child_by_field_name("object") else { return false; };
    let Some(prop) = func.child_by_field_name("property") else { return false; };
    let obj_txt = obj.utf8_text(source).unwrap_or("");
    let prop_txt = prop.utf8_text(source).unwrap_or("");
    matches!(obj_txt, "vi" | "jest") && prop_txt == "useFakeTimers"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Fast file-level pre-filter: if useRealTimers exists anywhere in the
    // file, assume it's the matching restore (can't disambiguate per-scope
    // cheaply here). Otherwise every useFakeTimers in the file is flagged.
    if ctx.source.contains("useRealTimers") { return; }
    if !ctx.source.contains("useFakeTimers") { return; }

    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if !is_use_fake_timers(func, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "useFakeTimers() without a matching useRealTimers() in afterEach/afterAll leaks mocked timers into sibling tests.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_vi_use_fake_timers_without_restore() {
        assert_eq!(
            run("beforeEach(() => { vi.useFakeTimers(); });").len(),
            1
        );
    }

    #[test]
    fn flags_jest_use_fake_timers_without_restore() {
        assert_eq!(
            run("beforeAll(() => { jest.useFakeTimers(); });").len(),
            1
        );
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
}
