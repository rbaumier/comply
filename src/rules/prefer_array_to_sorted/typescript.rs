use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "sort" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return; };

    let is_copy_pattern = match obj.kind() {
        // [...arr].sort()
        "array" => {
            let child_count = obj.named_child_count();
            child_count == 1 && obj.named_child(0).map(|c| c.kind()) == Some("spread_element")
        }
        // arr.slice().sort()
        "call_expression" => {
            if let Some(inner_func) = obj.child_by_field_name("function") {
                if inner_func.kind() == "member_expression" {
                    if let Some(inner_prop) = inner_func.child_by_field_name("property") {
                        inner_prop.utf8_text(source).unwrap_or("") == "slice"
                    } else { false }
                } else { false }
            } else { false }
        }
        _ => false,
    };

    if !is_copy_pattern { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-to-sorted".into(),
        message: "Use `arr.toSorted()` instead of copying then sorting (ES2023).".into(),
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
    fn flags_spread_sort() {
        assert_eq!(run("[...arr].sort()").len(), 1);
    }

    #[test]
    fn flags_slice_sort() {
        assert_eq!(run("arr.slice().sort()").len(), 1);
    }

    #[test]
    fn flags_slice_sort_with_comparator() {
        assert_eq!(run("arr.slice().sort((a, b) => a - b)").len(), 1);
    }

    #[test]
    fn allows_to_sorted() {
        assert!(run("arr.toSorted()").is_empty());
    }

    #[test]
    fn allows_mutating_sort() {
        assert!(run("arr.sort()").is_empty());
    }
}
