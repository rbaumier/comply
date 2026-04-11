use crate::diagnostic::{Diagnostic, Severity};

const WRAPPER_TYPES: &[&str] = &["String", "Number", "Boolean"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "new_expression" {
        return;
    }

    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.kind() != "identifier" {
        return;
    }

    let name = constructor.utf8_text(source).unwrap_or("");
    if !WRAPPER_TYPES.contains(&name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-primitive-wrappers".into(),
        message: format!(
            "Primitive wrapper object detected — `new {name}(...)` creates an object, not a primitive. Use `{name}(...)` without `new`.",
        ),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_string() {
        assert_eq!(run(r#"const s = new String("hello");"#).len(), 1);
    }

    #[test]
    fn flags_new_number() {
        assert_eq!(run("const n = new Number(42);").len(), 1);
    }

    #[test]
    fn flags_new_boolean() {
        assert_eq!(run("const b = new Boolean(true);").len(), 1);
    }

    #[test]
    fn allows_factory_calls() {
        assert!(run(r#"const s = String("hello");"#).is_empty());
        assert!(run("const n = Number(42);").is_empty());
        assert!(run("const b = Boolean(0);").is_empty());
    }

    #[test]
    fn allows_unrelated_new() {
        assert!(run("const m = new Map();").is_empty());
    }
}
