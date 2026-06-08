//! prefer-object-literal backend — flag `new Object()` and `Object.create(null)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression", "call_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // `new Object()`
        "new_expression" => {
            let Some(ctor) = node.child_by_field_name("constructor") else { return };
            if ctor.kind() != "identifier" { return; }
            if ctor.utf8_text(source).unwrap_or("") != "Object" { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-object-literal".into(),
                message: "Use `{}` instead of `new Object()`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // `Object.create(null)`
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "member_expression" { return; }

            let Some(obj) = func.child_by_field_name("object") else { return };
            let Some(prop) = func.child_by_field_name("property") else { return };

            if obj.utf8_text(source).unwrap_or("") != "Object" { return; }
            if prop.utf8_text(source).unwrap_or("") != "create" { return; }

            // Must have exactly one argument: `null`.
            let Some(args) = node.child_by_field_name("arguments") else { return };
            if args.named_child_count() != 1 { return; }
            let arg = args.named_child(0).unwrap();
            if arg.utf8_text(source).unwrap_or("") != "null" { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-object-literal".into(),
                message: "Prefer an object literal over `Object.create(null)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
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
    fn flags_new_object() {
        let d = run_on("const obj = new Object();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Object()"));
    }

    #[test]
    fn flags_object_create_null() {
        let d = run_on("const obj = Object.create(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.create(null)"));
    }

    #[test]
    fn allows_object_literal() {
        assert!(run_on("const obj = {};").is_empty());
    }

    #[test]
    fn allows_object_create_with_prototype() {
        assert!(run_on("const obj = Object.create(proto);").is_empty());
    }
}
