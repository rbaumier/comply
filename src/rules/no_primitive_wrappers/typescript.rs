use crate::diagnostic::{Diagnostic, Severity};

const WRAPPER_TYPES: &[&str] = &["String", "Number", "Boolean"];

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
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
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-primitive-wrappers".into(),
        message: format!(
            "Primitive wrapper object detected — `new {name}(...)` creates an object, not a primitive. Use `{name}(...)` without `new`.",
        ),
        severity: Severity::Error,
        span: None,
    });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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
