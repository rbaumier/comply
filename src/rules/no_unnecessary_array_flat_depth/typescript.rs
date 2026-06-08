//! no-unnecessary-array-flat-depth backend — flag `.flat(1)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Check that the callee is a member expression with property "flat".
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "flat" {
        return;
    }

    // Check arguments — must have exactly one argument that is `1`.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let arg_nodes: Vec<_> = args.children(&mut cursor)
        .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
        .collect();

    if arg_nodes.len() != 1 {
        return;
    }

    let arg = arg_nodes[0];
    if arg.kind() == "number" && arg.utf8_text(source).unwrap_or("") == "1" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-unnecessary-array-flat-depth".into(),
            message: "Passing `1` as the `depth` argument of `.flat()` is unnecessary \u{2014} it is the default.".into(),
            severity: Severity::Warning,
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_flat_one() {
        let d = crate::rules::test_helpers::run_rule(&Check, "arr.flat(1);", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-array-flat-depth");
    }

    #[test]
    fn allows_flat_no_args() {
        let d = crate::rules::test_helpers::run_rule(&Check, "arr.flat();", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_other_depth() {
        let d = crate::rules::test_helpers::run_rule(&Check, "arr.flat(2);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        let d = crate::rules::test_helpers::run_rule(&Check, "arr.flat(Infinity);", "t.ts");
        assert!(d.is_empty());
    }
}
