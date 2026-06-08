use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["splice"] => |node, source, ctx, diagnostics|
    // Look for .splice() call
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "splice" { return; }

    // Check if the object is a .slice() call or [...arr]
    let Some(obj) = func.child_by_field_name("object") else { return; };

    let is_copy_pattern = match obj.kind() {
        "call_expression" => {
            // arr.slice().splice()
            let Some(inner_func) = obj.child_by_field_name("function") else { return; };
            if inner_func.kind() != "member_expression" { return; }
            let Some(inner_prop) = inner_func.child_by_field_name("property") else { return; };
            inner_prop.utf8_text(source).unwrap_or("") == "slice"
        }
        "array" => {
            // [...arr].splice()
            obj.named_child_count() == 1
                && obj.named_child(0).map(|c| c.kind() == "spread_element").unwrap_or(false)
        }
        _ => false,
    };

    if !is_copy_pattern { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-to-spliced".into(),
        message: "Use `toSpliced()` instead of copy-then-splice pattern.".into(),
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_slice_splice() {
        assert_eq!(run("arr.slice().splice(1, 2)").len(), 1);
    }

    #[test]
    fn flags_spread_splice() {
        assert_eq!(run("[...arr].splice(1, 2)").len(), 1);
    }

    #[test]
    fn allows_direct_splice() {
        assert!(run("arr.splice(1, 2)").is_empty());
    }

    #[test]
    fn allows_to_spliced() {
        assert!(run("arr.toSpliced(1, 2)").is_empty());
    }
}
