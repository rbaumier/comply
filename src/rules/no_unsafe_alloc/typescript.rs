//! no-unsafe-alloc backend — flag `Buffer.allocUnsafe(...)`,
//! `Buffer.allocUnsafeSlow(...)`, and `new Buffer(size)` with a
//! numeric first argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression", "new_expression"] prefilter = ["Buffer"] => |node, source, ctx, diagnostics|
match node.kind() {
        "call_expression" => {
            let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
                return;
            };
            if name != "Buffer.allocUnsafe" && name != "Buffer.allocUnsafeSlow" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-unsafe-alloc".into(),
                message: format!("`{name}()` returns uninitialized memory — use `Buffer.alloc()` instead."),
                severity: Severity::Error,
                span: None,
            });
        }
        "new_expression" => {
            let Some(ctor) = node.child_by_field_name("constructor") else { return };
            let Ok(ctor_text) = ctor.utf8_text(source) else { return };
            if ctor_text != "Buffer" {
                return;
            }
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let Some(first) = args.named_child(0) else { return };
            // Flag numeric args (`new Buffer(10)`) and identifiers (potentially numeric).
            // `new Buffer("string")` or `new Buffer(array)` are not size-based.
            let kind = first.kind();
            if kind != "number" && kind != "identifier" && kind != "binary_expression" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-unsafe-alloc".into(),
                message: "`new Buffer(size)` is deprecated and returns uninitialized memory — use `Buffer.alloc(size)` instead.".into(),
                severity: Severity::Error,
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
    fn flags_buffer_alloc_unsafe() {
        assert_eq!(run_on("const b = Buffer.allocUnsafe(10);").len(), 1);
    }

    #[test]
    fn flags_buffer_alloc_unsafe_slow() {
        assert_eq!(run_on("const b = Buffer.allocUnsafeSlow(10);").len(), 1);
    }

    #[test]
    fn flags_new_buffer_with_size_literal() {
        assert_eq!(run_on("const b = new Buffer(10);").len(), 1);
    }

    #[test]
    fn flags_new_buffer_with_size_variable() {
        assert_eq!(run_on("const b = new Buffer(size);").len(), 1);
    }

    #[test]
    fn allows_buffer_alloc() {
        assert!(run_on("const b = Buffer.alloc(10);").is_empty());
    }

    #[test]
    fn allows_buffer_from() {
        assert!(run_on("const b = Buffer.from('hello');").is_empty());
    }
}
