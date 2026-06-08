//! pure-by-default Rust backend.
//!
//! Flag functions referencing `static mut` variables — Rust's equivalent
//! of top-level mutable state.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    // 1. Collect `static mut` variable names.
    let mut mutable_statics: Vec<String> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "static_item" {
            let Ok(text) = child.utf8_text(source) else { continue };
            if text.contains("static mut ")
                && let Some(name_node) = child.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(source) {
                    mutable_statics.push(name.to_string());
                }
        }
    }

    if mutable_statics.is_empty() {
        return;
    }

    // 2. Check functions for references to those statics.
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if child.kind() == "function_item" {
            let Some(name_node) = child.child_by_field_name("name") else { continue };
            let Ok(fn_name) = name_node.utf8_text(source) else { continue };
            let Some(body) = child.child_by_field_name("body") else { continue };
            let Ok(body_text) = body.utf8_text(source) else { continue };

            for static_var in &mutable_statics {
                if body_text.contains(static_var.as_str()) {
                    let pos = name_node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "pure-by-default".into(),
                        message: format!(
                            "Function `{fn_name}` references `static mut {static_var}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_static_mut_reference() {
        let src = "static mut COUNTER: i32 = 0;\nfn inc() { unsafe { COUNTER += 1; } }\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_no_static_mut() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
        assert!(run_on(src).is_empty());
    }
}
