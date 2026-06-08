//! prefer-array-find AST backend — flag `.filter(…)[0]`, `.filter(…).at(0)`,
//! and `.filter(…).shift()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression", "subscript_expression"] => |node, source, ctx, diagnostics|
    // Pattern 1: `.filter(…)[0]` — subscript_expression whose object is
    // a call_expression with method `filter` and index is `0`.
    if node.kind() == "subscript_expression" {
        let Some(obj) = node.child_by_field_name("object") else { return };
        let Some(idx) = node.child_by_field_name("index") else { return };

        if obj.kind() != "call_expression" {
            return;
        }
        if idx.utf8_text(source).unwrap_or("") != "0" {
            return;
        }

        let Some(callee) = obj.child_by_field_name("function") else { return };
        if callee.kind() != "member_expression" {
            return;
        }
        let Some(prop) = callee.child_by_field_name("property") else { return };
        if prop.utf8_text(source).unwrap_or("") != "filter" {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-array-find".into(),
            message: "Prefer `.find(…)` over `.filter(…)[0]` — `.find()` short-circuits on the first match.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    // Pattern 2: `.filter(…).at(0)` or `.filter(…).shift()`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.kind() != "call_expression" {
        return;
    }

    let Some(inner_callee) = obj.child_by_field_name("function") else { return };
    if inner_callee.kind() != "member_expression" {
        return;
    }
    let Some(inner_prop) = inner_callee.child_by_field_name("property") else { return };
    if inner_prop.utf8_text(source).unwrap_or("") != "filter" {
        return;
    }

    match method {
        "at" => {
            // Check that the argument is `0`.
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let first = args.children(&mut cursor)
                .find(|c| !matches!(c.kind(), "(" | ")" | ","));
            let Some(arg) = first else { return };
            if arg.utf8_text(source).unwrap_or("") != "0" {
                return;
            }
        }
        "shift" => { /* .filter(…).shift() — always flag */ }
        _ => return,
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-find".into(),
        message: "Prefer `.find(…)` over `.filter(…)[0]` — `.find()` short-circuits on the first match.".into(),
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
    fn flags_filter_zero_index() {
        assert_eq!(run_on("const x = arr.filter(fn)[0];").len(), 1);
    }

    #[test]
    fn flags_filter_at_zero() {
        assert_eq!(run_on("const x = arr.filter(fn).at(0);").len(), 1);
    }

    #[test]
    fn flags_filter_shift() {
        assert_eq!(run_on("const x = arr.filter(fn).shift();").len(), 1);
    }

    #[test]
    fn allows_find() {
        assert!(run_on("const x = arr.find(fn);").is_empty());
    }

    #[test]
    fn allows_filter_alone() {
        assert!(run_on("const x = arr.filter(fn);").is_empty());
    }

    #[test]
    fn allows_filter_non_zero_index() {
        assert!(run_on("const x = arr.filter(fn)[1];").is_empty());
    }
}
