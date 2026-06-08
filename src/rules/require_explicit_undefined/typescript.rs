//! Implementation for the require-explicit-undefined rule (TS family).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["return_statement"] => |node, source, ctx, diagnostics|
    // A `return;` with no argument has no named child. `return expr;` has the
    // expression as a named child.
    if node.named_child(0).is_some() { return; }

    // Walk up to the nearest enclosing function-like node.
    let mut cur = node.parent();
    let func = loop {
        let Some(n) = cur else { return; };
        match n.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "method_signature"
            | "function_signature" => break n,
            "class_declaration" | "program" => return,
            _ => cur = n.parent(),
        }
    };

    // Constructors have name.text == "constructor" and no meaningful return type.
    if func.kind() == "method_definition"
        && let Some(name) = func.child_by_field_name("name")
        && let Ok(text) = name.utf8_text(source)
        && text == "constructor"
    {
        return;
    }

    // Return type annotation lives under field name `return_type`.
    let Some(ret_type) = func.child_by_field_name("return_type") else { return; };
    let Ok(ret_text) = ret_type.utf8_text(source) else { return; };
    let trimmed = ret_text.trim_start_matches(':').trim();

    if trimmed == "void" || trimmed == "never" { return; }
    if trimmed == "Promise<void>" || trimmed == "Promise<never>" { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "require-explicit-undefined",
        "Bare `return;` in a function that returns a value — use `return undefined;` for clarity.".into(),
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_return_in_optional_return() {
        let src = "function getUser(): User | undefined { return; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_return_in_undefined_only() {
        let src = "function nothing(): undefined { return; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_bare_return_in_void() {
        let src = "function sideEffect(): void { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_return_in_never() {
        let src = "function bail(): never { throw new Error('x'); return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_with_value() {
        let src = "function x(): number { return 1; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_in_constructor() {
        let src = "class C { constructor() { return; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_return_without_annotation() {
        let src = "function x() { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_promise_void() {
        let src = "async function x(): Promise<void> { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_in_arrow_function_with_block() {
        let src = "const f = (): string | undefined => { if (x) return; return 'x'; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_in_method_with_return_type() {
        let src = "class C { find(): Item | undefined { return; } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
