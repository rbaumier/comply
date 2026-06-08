//! xstate-no-inline-implementation — flag inline functions assigned to XState
//! `actions`, `entry`, `exit`, `guard`, `cond`, or invoke `src` keys.
//!
//! Inline arrow functions or function expressions force implementations to
//! live inside the machine definition, which prevents reuse and makes them
//! harder to test in isolation. XState supports named references resolved
//! through the `actions`/`guards`/`services` options on `setup`/`createMachine`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] prefilter = ["xstate"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    if !matches!(
        key_text,
        "actions" | "entry" | "exit" | "guard" | "cond" | "src"
    ) {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if !matches!(value.kind(), "arrow_function" | "function_expression") {
        return;
    }

    // Walk ancestors: only emit if inside a createMachine / setup call.
    let mut cur = node.parent();
    let mut inside_machine = false;
    while let Some(p) = cur {
        if p.kind() == "call_expression" {
            if let Some(callee) = p.child_by_field_name("function") {
                let callee_text = callee.utf8_text(source).unwrap_or("");
                if callee_text.contains("createMachine") || callee_text.contains("setup") {
                    inside_machine = true;
                    break;
                }
            }
        }
        cur = p.parent();
    }
    if !inside_machine {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Inline function used as `{key_text}` — define it as a named action/guard/service instead."
        ),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_inline_entry_action() {
        let src = r#"
            createMachine({
                entry: () => console.log("hi"),
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_exit_function_expression() {
        let src = r#"
            createMachine({
                exit: function () { doStuff(); },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_guard() {
        let src = r#"
            createMachine({
                on: { EVENT: { guard: (ctx, e) => ctx.ok } },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_cond_legacy_name() {
        let src = r#"
            createMachine({
                on: { EVENT: { cond: () => true } },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_invoke_src() {
        let src = r#"
            createMachine({
                invoke: { src: () => fetch("/api") },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_named_string_action() {
        let src = r#"
            createMachine({
                entry: "logIt",
                exit: "cleanup",
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_named_guard_reference() {
        let src = r#"
            createMachine({
                on: { EVENT: { guard: "isReady" } },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_invoke_src_string() {
        let src = r#"
            createMachine({
                invoke: { src: "fetchUser" },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_inline_entry_outside_machine() {
        let src = r#"
            import { createMachine } from 'xstate';
            const uiConfig = { entry: () => openPanel() };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_inline_entry_inside_create_machine() {
        let src = r#"
            import { createMachine } from 'xstate';
            createMachine({ entry: () => openPanel() });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_inside_setup() {
        let src = r#"
            import { setup } from 'xstate';
            setup({}).createMachine({
                entry: () => console.log("hi"),
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
