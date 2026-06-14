use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ImportDeclarationSpecifier};
use oxc_span::GetSpan;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

/// Substrings that signal the file drives fake timers. When present,
/// `fireEvent` (synchronous) is the correct tool — `userEvent` is async and
/// flushes microtasks/pointer events, which breaks deterministic interleaving
/// with `vi.advanceTimersByTimeAsync(...)` and friends. `advanceTimersByTime`
/// also matches its `...Async` variant.
const FAKE_TIMER_MARKERS: &[&str] = &[
    "useFakeTimers",
    "advanceTimersByTime",
    "advanceTimersToNextTimer",
    "runOnlyPendingTimers",
    "runAllTimers",
];

/// True when the `fireEvent` binding in this file is imported from a relative
/// module specifier (starts with `.` / `..`), i.e. the file tests a *local*
/// `fireEvent` implementation rather than the published package. The source
/// repository of a library that exports `fireEvent` (e.g. `@testing-library/react`)
/// imports it from `'../'` / `'./fire-event'` to test its own behavior — there
/// `userEvent` cannot replace `fireEvent`, which is the code under test.
///
/// A `fireEvent` imported from a bare package specifier (`@testing-library/react`,
/// `@testing-library/dom`) is the rule's genuine target and returns `false`.
fn fire_event_imported_from_relative(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        let spec = decl.source.value.as_str();
        if !(spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == "..") {
            return false;
        }
        let Some(specifiers) = &decl.specifiers else {
            return false;
        };
        specifiers.iter().any(|s| {
            let local = match s {
                ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => &def.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => &ns.local,
            };
            local.name.as_str() == "fireEvent"
        })
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "fireEvent" {
            return;
        }
        if member.property.name.as_str() != "click" {
            return;
        }
        let path_str = ctx.path.to_string_lossy();
        if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
            return;
        }
        // Files driving fake timers rely on `fireEvent` being synchronous to
        // interleave precisely with timer advancement; `userEvent` would break
        // that, so leave them alone.
        if FAKE_TIMER_MARKERS
            .iter()
            .any(|m| ctx.source_contains(m))
        {
            return;
        }
        // The source repository of a library that exports `fireEvent` imports it
        // from a relative path to test its own implementation; `userEvent` cannot
        // replace the code under test, so leave those files alone.
        if fire_event_imported_from_relative(semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `userEvent.click` over `fireEvent.click` — `fireEvent.click` dispatches a single synthetic click and skips the pointer/focus events a real browser fires.".into(),
            severity: super::META.severity,
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

    fn run_on(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_fire_event_in_test() {
        let diags = run_on(
            "components/__tests__/button.test.tsx",
            "fireEvent.click(button)",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_user_event() {
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "userEvent.click(button)"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_on("components/button.tsx", "fireEvent.click(button)").is_empty());
    }

    #[test]
    fn allows_fire_event_focus() {
        assert!(
            run_on(
                "components/__tests__/combobox.test.tsx",
                "fireEvent.focus(screen.getByRole(\"combobox\"))",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_blur() {
        assert!(
            run_on("components/__tests__/input.test.tsx", "fireEvent.blur(el)").is_empty()
        );
    }

    #[test]
    fn allows_fire_event_key_down() {
        assert!(
            run_on(
                "components/__tests__/input.test.tsx",
                "fireEvent.keyDown(el, { key: \"Enter\" })",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_change() {
        assert!(
            run_on(
                "components/__tests__/input.test.tsx",
                "fireEvent.change(el, { target: { value: \"x\" } })",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_pointer_down() {
        assert!(
            run_on(
                "components/__tests__/popover.test.tsx",
                "fireEvent.pointerDown(el)",
            )
            .is_empty()
        );
    }

    #[test]
    fn no_flag_bare_reference_in_foreach() {
        // fireEvent.click passed as a callback — not an invocation
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "array.forEach(fireEvent.click)",
            )
            .is_empty()
        );
    }

    #[test]
    fn no_flag_bare_reference_assigned() {
        // fireEvent.click assigned to a variable — not an invocation
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "const handler = fireEvent.click;",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_when_fake_timers_advanced() {
        // Regression #1278: fireEvent (sync) is the right tool when the file
        // interleaves clicks with fake-timer advancement; userEvent (async)
        // would break the deterministic timing.
        let src = "\
            fireEvent.click(rendered.getByRole('button', { name: /mutate1/i }))\n\
            await vi.advanceTimersByTimeAsync(10)\n\
            fireEvent.click(rendered.getByRole('button', { name: /mutate2/i }))\n";
        assert!(
            run_on("preact-query/src/__tests__/useMutationState.test.tsx", src).is_empty()
        );
    }

    #[test]
    fn allows_fire_event_when_use_fake_timers() {
        let src = "\
            beforeEach(() => { vi.useFakeTimers(); });\n\
            fireEvent.click(button)\n";
        assert!(run_on("components/__tests__/button.test.tsx", src).is_empty());
    }

    #[test]
    fn still_flags_fire_event_without_fake_timers() {
        // Negative-space guard: no fake-timer usage → userEvent is genuinely
        // preferred, keep flagging.
        let src = "\
            it('clicks', () => { fireEvent.click(button) })\n";
        assert_eq!(
            run_on("components/__tests__/button.test.tsx", src).len(),
            1
        );
    }

    #[test]
    fn allows_fire_event_imported_from_parent_relative() {
        // Regression #2173: the source repo of a library that exports `fireEvent`
        // imports it from a relative path to test its own implementation; the test
        // asserts on `fireEvent` behavior, so `userEvent` cannot replace it.
        let src = "\
            import { render, fireEvent } from '../'\n\
            test('hydrate', () => { fireEvent.click(container.querySelector('button')) })\n";
        assert!(run_on("src/__tests__/render.js", src).is_empty());
    }

    #[test]
    fn allows_fire_event_imported_from_sibling_module() {
        let src = "\
            import { fireEvent } from './fire-event'\n\
            fireEvent.click(getByTestId('button'))\n";
        assert!(run_on("src/__tests__/act.js", src).is_empty());
    }

    #[test]
    fn still_flags_fire_event_imported_from_published_package() {
        // Negative-space guard: an application importing `fireEvent` from the
        // published package is the rule's genuine target — keep flagging.
        let src = "\
            import { render, fireEvent } from '@testing-library/react'\n\
            it('clicks', () => { fireEvent.click(button) })\n";
        assert_eq!(run_on("components/__tests__/button.test.tsx", src).len(), 1);
    }
}
