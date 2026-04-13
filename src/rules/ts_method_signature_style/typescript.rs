//! ts-method-signature-style backend — flag shorthand method signatures
//! in interfaces and type literals.
//!
//! Detection: walk `method_signature` nodes inside `interface_body` or
//! `object_type` — these represent the shorthand `foo(): void` form.
//! Property signatures with function types (`foo: () => void`) use
//! `property_signature` instead, which we allow.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "method_signature" {
        return;
    }

    // Only flag inside interface bodies and type literals.
    let Some(parent) = node.parent() else { return };
    let pk = parent.kind();
    if pk != "interface_body" && pk != "object_type" {
        return;
    }

    // Get method name for the message.
    let name = node.child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("method");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-method-signature-style".into(),
        message: format!(
            "Shorthand method signature `{name}(...)` is less safe — \
             use a property signature: `{name}: (...) => ReturnType`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_method_signature() {
        let diags = run_on("interface Foo { bar(x: string): void; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_property_signature() {
        assert!(run_on("interface Foo { bar: (x: string) => void; }").is_empty());
    }

    #[test]
    fn flags_in_type_literal() {
        let diags = run_on("type Foo = { bar(): void; };");
        assert_eq!(diags.len(), 1);
    }
}
