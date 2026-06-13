//! Flag `foo(a, b) { return this.bar(a, b); }` — a method whose body is a
//! single `return` forwarding the exact parameters to another callee with no
//! transformation.

use crate::diagnostic::{Diagnostic, Severity};

fn param_names<'a>(params: &tree_sitter::Node<'a>, source: &'a [u8]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..params.named_child_count() {
        let Some(child) = params.named_child(i) else {
            continue;
        };
        let name_node = match child.kind() {
            "required_parameter" | "optional_parameter" => child.child_by_field_name("pattern"),
            "identifier" => Some(child),
            _ => None,
        };
        if let Some(n) = name_node
            && let Ok(text) = n.utf8_text(source)
        {
            out.push(text.to_string());
        }
    }
    out
}

fn argument_names<'a>(args: &tree_sitter::Node<'a>, source: &'a [u8]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for i in 0..args.named_child_count() {
        let Some(child) = args.named_child(i) else {
            continue;
        };
        if child.kind() != "identifier" {
            return None;
        }
        out.push(child.utf8_text(source).ok()?.to_string());
    }
    Some(out)
}

fn has_decorator(method: &tree_sitter::Node) -> bool {
    let mut cursor = method.walk();
    if method.named_children(&mut cursor).any(|c| c.kind() == "decorator") {
        return true;
    }
    // In the TS grammar a method's decorators are emitted as preceding
    // siblings inside the `class_body`, not as children of the method.
    let Some(parent) = method.parent() else { return false };
    let mut cursor = parent.walk();
    let mut decorator_before_current = false;
    for child in parent.named_children(&mut cursor) {
        if child.kind() == "decorator" {
            decorator_before_current = true;
            continue;
        }
        if child.start_byte() == method.start_byte() && child.end_byte() == method.end_byte() {
            return decorator_before_current;
        }
        decorator_before_current = false;
    }
    false
}

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    // A decorated method carries external significance beyond its body: the
    // decorator binds it to a framework (e.g. NestJS `@MessagePattern` /
    // `@EventPattern` / `@Get`) that resolves the method via metadata
    // reflection at runtime. The forwarding body cannot be inlined or removed
    // without breaking that registration, so the passthrough is intentional.
    if has_decorator(&node) { return; }
    let Some(params) = node.child_by_field_name("parameters") else { return };
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" { return; }
    // Body must contain exactly one named child, a return_statement.
    if body.named_child_count() != 1 { return; }
    let Some(stmt) = body.named_child(0) else { return };
    if stmt.kind() != "return_statement" { return; }
    let Some(expr) = stmt.named_child(0) else { return };
    if expr.kind() != "call_expression" { return; }
    let Some(args) = expr.child_by_field_name("arguments") else { return };
    let Some(arg_names) = argument_names(&args, source) else { return };
    let params = param_names(&params, source);
    if params.is_empty() { return; }
    if params != arg_names { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Method is a pure pass-through — forwards the same arguments with no added logic. Inline the call or remove the indirection.".into(),
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_passthrough() {
        let src = "class A { foo(a, b) { return this.bar(a, b); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reordered_args() {
        let src = "class A { foo(a, b) { return this.bar(b, a); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_added_logic() {
        let src = "class A { foo(a, b) { const x = a + 1; return this.bar(x, b); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_decorated_message_handler() {
        // Regression for #2020: a NestJS `@MessagePattern` handler forwards its
        // parameter but the decorator registers it as an RPC entry point.
        let src = "class NatsController { @MessagePattern('streaming.*') streaming(data) { return from(data); } }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }
}
