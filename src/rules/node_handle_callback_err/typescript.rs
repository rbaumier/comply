//! node-handle-callback-err backend — flag callback error params that are never used.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a parameter name looks like an error parameter.
fn is_error_param(name: &str) -> bool {
    name == "err" || name == "error" || name == "e"
}

/// Check if the function body text (between braces) references the given
/// parameter name as a standalone identifier.
fn body_uses_param(body_text: &str, param_name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = body_text[start..].find(param_name) {
        let abs = start + pos;
        let before_ok = abs == 0 || {
            let prev = body_text.as_bytes()[abs - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        let after_ok = {
            let after = abs + param_name.len();
            after >= body_text.len() || {
                let next = body_text.as_bytes()[after];
                !next.is_ascii_alphanumeric() && next != b'_'
            }
        };
        if before_ok && after_ok {
            return true;
        }
        start = abs + param_name.len();
    }
    false
}

crate::ast_check! { on ["function_declaration", "function", "arrow_function"] => |node, source, ctx, diagnostics|
    // Match function declarations, function expressions, and arrow functions.
    // Get the formal parameters node.
    let Some(params_node) = node.child_by_field_name("parameters") else {
        return;
    };

    // Find the first parameter.
    let mut first_param = None;
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        let ck = child.kind();
        if ck == "identifier"
            || ck == "required_parameter"
            || ck == "optional_parameter"
        {
            first_param = Some(child);
            break;
        }
    }

    let Some(param) = first_param else { return };

    // Extract the parameter name. For `required_parameter` / `optional_parameter`,
    // the name is in a child `identifier` or `pattern` field.
    let param_name = match param.kind() {
        "identifier" => {
            param.utf8_text(source).unwrap_or("")
        }
        "required_parameter" | "optional_parameter" => {
            let name_node = param.child_by_field_name("pattern")
                .or_else(|| {
                    let mut c = param.walk();
                    param.children(&mut c).find(|ch| ch.kind() == "identifier")
                });
            match name_node {
                Some(n) => n.utf8_text(source).unwrap_or(""),
                None => return,
            }
        }
        _ => return,
    };

    if !is_error_param(param_name) {
        return;
    }

    // Prefixed with `_` means intentionally unused.
    if param_name.starts_with('_') {
        return;
    }

    // Get the function body and check if the param is referenced inside it.
    let Some(body) = node.child_by_field_name("body") else { return };
    let body_text = body.utf8_text(source).unwrap_or("");

    if !body_uses_param(body_text, param_name) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "node-handle-callback-err".into(),
            message: format!("Callback error parameter `{param_name}` is declared but never used — handle the error."),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unused_err_param() {
        let d = run_on("function handle(err, data) { console.log(data); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("err"));
    }

    #[test]
    fn flags_unused_error_param() {
        let d = run_on("const fn = (error, result) => { return result; };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("error"));
    }

    #[test]
    fn allows_used_err_param() {
        assert!(run_on("function handle(err, data) { if (err) throw err; }").is_empty());
    }

    #[test]
    fn allows_used_error_param_in_arrow() {
        assert!(run_on("const fn = (error) => { console.error(error); };").is_empty());
    }

    #[test]
    fn allows_non_error_param() {
        assert!(run_on("function handle(result) { return result; }").is_empty());
    }

    #[test]
    fn allows_underscore_prefix() {
        // _err is intentionally unused — should not be flagged (but our check
        // only matches "err", "error", "e" — `_err` doesn't match).
        assert!(run_on("function handle(_err, data) { return data; }").is_empty());
    }
}
