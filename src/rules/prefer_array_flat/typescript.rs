//! prefer-array-flat AST backend — flag `[].concat(...arr)` and
//! `.reduce((a, b) => a.concat(b), [])` patterns.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");

    match method {
        "concat" => {
            // Check for `[].concat(...arr)` pattern.
            let Some(obj) = callee.child_by_field_name("object") else { return };
            if obj.kind() != "array" {
                return;
            }
            // Empty array: no children other than `[` and `]`.
            let mut c = obj.walk();
            let has_elements = obj.children(&mut c)
                .any(|ch| !matches!(ch.kind(), "[" | "]"));
            if has_elements {
                return;
            }

            // First argument should be a spread element.
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let first = args.children(&mut cursor)
                .find(|c| !matches!(c.kind(), "(" | ")" | ","));
            let Some(arg) = first else { return };
            if arg.kind() != "spread_element" {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-array-flat".into(),
                message: "Prefer `.flat()` over legacy array flattening patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        "reduce" => {
            // Check for `.reduce((a, b) => a.concat(b), [])` or
            // `.reduce((a, b) => [...a, ...b], [])` pattern.
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let real_args: Vec<_> = args.children(&mut cursor)
                .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
                .collect();

            // Need exactly 2 args: callback and initial value `[]`.
            if real_args.len() != 2 {
                return;
            }
            let callback = real_args[0];
            let init = real_args[1];

            // Initial value must be empty array `[]`.
            if init.kind() != "array" {
                return;
            }
            let mut ic = init.walk();
            let has_elements = init.children(&mut ic)
                .any(|ch| !matches!(ch.kind(), "[" | "]"));
            if has_elements {
                return;
            }

            // Callback body should contain `.concat(` or spread `[...`.
            let body_text = callback.utf8_text(source).unwrap_or("");
            if !body_text.contains(".concat(") && !body_text.contains("[...") {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-array-flat".into(),
                message: "Prefer `.flat()` over legacy array flattening patterns.".into(),
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
    fn flags_empty_concat_spread() {
        assert_eq!(run_on("const flat = [].concat(...arr);").len(), 1);
    }

    #[test]
    fn flags_reduce_concat() {
        assert_eq!(
            run_on("const flat = arr.reduce((a, b) => a.concat(b), []);").len(),
            1
        );
    }

    #[test]
    fn flags_reduce_spread() {
        assert_eq!(
            run_on("const flat = arr.reduce((a, b) => [...a, ...b], []);").len(),
            1
        );
    }

    #[test]
    fn allows_flat() {
        assert!(run_on("const flat = arr.flat();").is_empty());
    }

    #[test]
    fn allows_concat_without_spread() {
        assert!(run_on("const merged = [].concat(arr);").is_empty());
    }

    #[test]
    fn allows_reduce_without_empty_init() {
        assert!(run_on("const sum = arr.reduce((a, b) => a + b, 0);").is_empty());
    }
}
