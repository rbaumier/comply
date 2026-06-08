//! zod-no-string-schema-with-uuid backend — flag `z.string().uuid()`.
//!
//! Zod v4 exposes a dedicated top-level `z.uuid()` schema. The chained
//! `z.string().uuid()` form is deprecated because it composes a generic
//! string validator with a format refinement, which costs an extra
//! allocation and loses the narrower TypeScript branding that
//! `z.uuid()` ships with.
//!
//! Detection: walk `call_expression` nodes and match when the callee is
//! a `member_expression` whose property is `uuid` and whose object is
//! itself a `call_expression` calling `z.string()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["z.string"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }

    let Some(property) = callee.child_by_field_name("property") else { return; };
    let Ok(prop_text) = property.utf8_text(source) else { return; };
    if prop_text != "uuid" { return; }

    let Some(object) = callee.child_by_field_name("object") else { return; };
    if object.kind() != "call_expression" { return; }

    let Some(inner_fn) = object.child_by_field_name("function") else { return; };
    let Ok(inner_text) = inner_fn.utf8_text(source) else { return; };
    if inner_text != "z.string" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-string-schema-with-uuid".into(),
        message: "Use `z.uuid()` instead of `z.string().uuid()` — the \
                  chained form is deprecated in Zod v4."
            .into(),
        severity: Severity::Warning,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_z_string_uuid() {
        assert_eq!(run_on("const s = z.string().uuid();").len(), 1);
    }

    #[test]
    fn allows_z_uuid() {
        assert!(run_on("const s = z.uuid();").is_empty());
    }

    #[test]
    fn allows_z_string_email() {
        assert!(run_on("const s = z.string().email();").is_empty());
    }

    #[test]
    fn flags_in_object_schema() {
        let src = "const User = z.object({ id: z.string().uuid() });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_bare_z_string() {
        assert!(run_on("const s = z.string();").is_empty());
    }
}
