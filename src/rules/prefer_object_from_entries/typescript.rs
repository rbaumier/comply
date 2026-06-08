//! prefer-object-from-entries backend — flag `.reduce(…, {})` building objects.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Must be a `.reduce(` call.
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(property) = func.child_by_field_name("property") else { return };
    if property.utf8_text(source).unwrap_or("") != "reduce" { return; }

    // Must have 2 arguments: callback and initial value.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let named_count = args.named_child_count();
    if named_count != 2 { return; }

    // Check the second argument (initial value).
    let init = args.named_child(1).unwrap();
    let is_empty_object = match init.kind() {
        // `{}`
        "object" => init.named_child_count() == 0,
        // `Object.create(null)`
        "call_expression" => {
            let Some(f) = init.child_by_field_name("function") else { return };
            if f.kind() != "member_expression" { return };
            let obj = f.child_by_field_name("object");
            let prop = f.child_by_field_name("property");
            let is_object_create = obj.is_some_and(|o| o.utf8_text(source).unwrap_or("") == "Object")
                && prop.is_some_and(|p| p.utf8_text(source).unwrap_or("") == "create");
            if !is_object_create { return };

            // Check argument is `null`.
            let Some(inner_args) = init.child_by_field_name("arguments") else { return };
            inner_args.named_child_count() == 1
                && inner_args.named_child(0).is_some_and(|a| a.utf8_text(source).unwrap_or("") == "null")
        }
        _ => false,
    };

    if !is_empty_object { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-object-from-entries".into(),
        message: "Prefer `Object.fromEntries()` over `Array#reduce()` to build an object.".into(),
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
    fn flags_reduce_with_empty_object() {
        let d = run_on("const obj = pairs.reduce((acc, [k, v]) => ({ ...acc, [k]: v }), {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reduce_with_object_create_null() {
        let d = run_on(
            "const obj = pairs.reduce((acc, [k, v]) => { acc[k] = v; return acc; }, Object.create(null));",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_reduce_with_non_object_init() {
        assert!(run_on("const sum = nums.reduce((acc, n) => acc + n, 0);").is_empty());
    }

    #[test]
    fn allows_object_from_entries() {
        assert!(
            run_on("const obj = Object.fromEntries(pairs.map(([k, v]) => [k, v]));").is_empty()
        );
    }

    #[test]
    fn allows_reduce_with_array_init() {
        assert!(run_on("const arr = items.reduce((acc, x) => [...acc, x], []);").is_empty());
    }
}
