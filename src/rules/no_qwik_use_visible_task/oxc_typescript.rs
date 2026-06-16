//! no-qwik-use-visible-task oxc backend.
//!
//! Mirrors Biome `noQwikUseVisibleTask`. Biome queries `JsCallExpression` and
//! fires when the callee resolves through `global_identifier` to the bare name
//! `useVisibleTask$` — the distinctive Qwik hook. There is no import-source
//! check: the `$`-suffixed name is the signal, and `global_identifier` ensures
//! the callee is an unqualified *global* reference, so a locally declared or
//! shadowed `useVisibleTask$` (or a member call like `qwik.useVisibleTask$()`)
//! is left alone.
//!
//! The single exemption: a second argument object literal carrying
//! `strategy: 'document-idle'` opts into idle scheduling, which Biome accepts.

use std::sync::Arc;

use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

/// The distinctive Qwik hook name. The trailing `$` makes it a QRL marker that
/// is effectively unique to Qwik, which is why Biome gates on the name alone.
const HOOK_NAME: &str = "useVisibleTask$";

/// Returns `true` when the 2nd argument is an object literal containing
/// `strategy: 'document-idle'`, the form Biome exempts.
fn has_idle_strategy(call: &oxc_ast::ast::CallExpression) -> bool {
    let Some(second) = call.arguments.get(1).and_then(Argument::as_expression) else {
        return false;
    };
    let Expression::ObjectExpression(obj) = second else {
        return false;
    };
    obj.properties.iter().any(|member| {
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            return false;
        };
        let key_is_strategy = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str() == "strategy",
            PropertyKey::StringLiteral(s) => s.value.as_str() == "strategy",
            _ => false,
        };
        key_is_strategy
            && matches!(&prop.value, Expression::StringLiteral(s) if s.value.as_str() == "document-idle")
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path requires a literal `useVisibleTask$` call in the
        // source; files without the substring are skipped before parsing.
        Some(&[HOOK_NAME])
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

        // Bare-identifier callee only — a member call (`qwik.useVisibleTask$()`)
        // is not a `global_identifier` in Biome and is out of scope.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != HOOK_NAME {
            return;
        }

        // `global_identifier` resolves only unqualified *global* references; a
        // locally declared/shadowed `useVisibleTask$` is not flagged.
        if !semantic.is_reference_to_global_variable(callee) {
            return;
        }

        if has_idle_strategy(call) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, callee.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid `useVisibleTask$` for non-interactive initialization. It runs eagerly on mount, blocking hydration and breaking Qwik's resumability.".into(),
            severity: Severity::Error,
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "src/component.tsx")
    }

    // ── Biome `invalid.js` fixtures: every bare `useVisibleTask$(...)` call
    //    fires, regardless of nesting or argument shape ────────────────────────

    #[test]
    fn flags_bare_call() {
        let diags = run_on("useVisibleTask$(() => {\n  console.log('visible');\n});");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_call_with_cleanup_param() {
        let src = "useVisibleTask$(({ cleanup }) => {\n  const s = obs.subscribe();\n  cleanup(() => s.unsubscribe());\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_eagerness_visible_option() {
        // Second arg is an object but NOT `strategy: 'document-idle'` → still fires.
        let src = "useVisibleTask$(() => {\n  document.title = 'x';\n}, { eagerness: 'visible' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_inside_component() {
        let src = "const C = component$(() => {\n  useVisibleTask$(() => { console.log('mounted'); });\n  return <div>Hello</div>;\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_inside_function_and_arrow_and_constructor() {
        let src = "\
function setupComponent() {\n  useVisibleTask$(() => {});\n}\n\
const setup = () => {\n  useVisibleTask$(() => {});\n};\n\
class MyClass {\n  constructor() {\n    useVisibleTask$(() => {});\n  }\n}";
        assert_eq!(run_on(src).len(), 3);
    }

    #[test]
    fn flags_each_of_multiple_calls() {
        let src = "useVisibleTask$(() => { console.log('1'); });\nuseVisibleTask$(() => { console.log('2'); });";
        assert_eq!(run_on(src).len(), 2);
    }

    // ── Biome `valid.js` fixtures: never fires ───────────────────────────────

    #[test]
    fn allows_different_hook_names() {
        let src = "useTask$(() => {});\nuseResource$(() => fetch('/api'));\nuseSignal(0);\nuseStore({ count: 0 });";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    #[test]
    fn allows_name_without_dollar() {
        // `useVisibleTask` (no `$`) is a different identifier.
        assert!(run_on("useVisibleTask(() => {});").is_empty());
    }

    #[test]
    fn allows_idle_strategy() {
        // The one exemption: `strategy: 'document-idle'`.
        assert!(run_on("useVisibleTask$(() => {}, { strategy: 'document-idle' });").is_empty());
    }

    #[test]
    fn allows_assignment_not_call() {
        // `useVisibleTask$ = () => {}` is an assignment, not a call.
        assert!(run_on("useVisibleTask$ = () => {};").is_empty());
    }

    #[test]
    fn allows_declarations_not_calls() {
        let src = "const useVisibleTask$ = () => {};\nfunction other() {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_string_literal_with_name() {
        assert!(run_on("const text = 'useVisibleTask$';").is_empty());
    }

    #[test]
    fn allows_react_hooks() {
        let src = "useEffect(() => {});\nuseState(0);\nuseCallback(() => {});";
        assert!(run_on(src).is_empty());
    }

    // ── Global-reference gate (Biome's `global_identifier`) ──────────────────

    #[test]
    fn allows_locally_declared_then_called() {
        // A local `const useVisibleTask$` is not a global reference, so calling
        // it is not flagged — mirrors `global_identifier`.
        let src = "const useVisibleTask$ = () => {};\nuseVisibleTask$();";
        assert!(run_on(src).is_empty(), "unexpected: {:?}", run_on(src));
    }

    #[test]
    fn allows_member_call() {
        // `qwik.useVisibleTask$()` is a member expression, not a global identifier.
        assert!(run_on("qwik.useVisibleTask$(() => {});").is_empty());
    }

    // ── Idle-strategy exemption boundaries ───────────────────────────────────

    #[test]
    fn flags_idle_strategy_string_key() {
        // Other strategy values do not exempt.
        assert!(!run_on("useVisibleTask$(() => {}, { strategy: 'visible' });").is_empty());
    }

    #[test]
    fn allows_idle_strategy_with_quoted_key() {
        assert!(run_on("useVisibleTask$(() => {}, { 'strategy': 'document-idle' });").is_empty());
    }
}
