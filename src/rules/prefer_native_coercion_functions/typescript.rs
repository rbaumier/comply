//! prefer-native-coercion-functions backend — flag `x => Number(x)` wrappers.

use crate::diagnostic::{Diagnostic, Severity};

/// Coercion functions that can be passed directly.
const COERCION_FUNCTIONS: &[&str] = &["String", "Number", "BigInt", "Boolean", "Symbol"];

crate::ast_check! { on ["arrow_function"] => |node, source, ctx, diagnostics|
    // Look for arrow functions: `x => Number(x)`
    // Must have exactly one parameter (simple identifier).
    // tree-sitter uses field "parameter" (singular) for bare arrow params `x => ...`
    // and "parameters" (plural) for parenthesized `(x) => ...`.
    let params = node.child_by_field_name("parameters")
        .or_else(|| node.child_by_field_name("parameter"));
    let Some(params) = params else { return };

    let param_name = match params.kind() {
        "identifier" => {
            // bare param: `x => ...`
            params.utf8_text(source).unwrap_or("")
        }
        "formal_parameters" => {
            // `(x) => ...` — must have exactly one child that is an identifier
            // or a required_parameter with an identifier pattern.
            let mut param = None;
            let mut count = 0;
            for i in 0..params.named_child_count() {
                let child = params.named_child(i).unwrap();
                count += 1;
                if count > 1 { return; }
                match child.kind() {
                    "identifier" => {
                        param = Some(child.utf8_text(source).unwrap_or(""));
                    }
                    "required_parameter" => {
                        if let Some(pattern) = child.child_by_field_name("pattern")
                            && pattern.kind() == "identifier" {
                                param = Some(pattern.utf8_text(source).unwrap_or(""));
                            }
                    }
                    _ => return,
                }
            }
            match param {
                Some(p) if count == 1 => p,
                _ => return,
            }
        }
        _ => return,
    };

    if param_name.is_empty() { return; }

    // Body must be a single call expression: `Number(x)`
    let Some(body) = node.child_by_field_name("body") else { return };

    // Handle block body with single return statement
    let call = if body.kind() == "statement_block" {
        // Look for a single return statement
        let mut ret_expr = None;
        let mut stmt_count = 0;
        for i in 0..body.named_child_count() {
            let child = body.named_child(i).unwrap();
            if child.kind() == "return_statement" {
                stmt_count += 1;
                // The return value is the first named child
                ret_expr = child.named_child(0);
            } else {
                stmt_count += 1;
            }
        }
        if stmt_count != 1 { return; }
        match ret_expr {
            Some(e) if e.kind() == "call_expression" => e,
            _ => return,
        }
    } else if body.kind() == "call_expression" {
        body
    } else {
        return;
    };

    let Some(func) = call.child_by_field_name("function") else { return };
    if func.kind() != "identifier" { return; }
    let func_name = func.utf8_text(source).unwrap_or("");

    if !COERCION_FUNCTIONS.contains(&func_name) { return; }

    // The call must have exactly one argument matching the parameter name.
    let Some(args) = call.child_by_field_name("arguments") else { return };
    let mut arg_count = 0;
    let mut first_arg_name = "";
    for i in 0..args.named_child_count() {
        let arg = args.named_child(i).unwrap();
        if arg_count == 0 && arg.kind() == "identifier" {
            first_arg_name = arg.utf8_text(source).unwrap_or("");
        }
        arg_count += 1;
    }

    if arg_count != 1 || first_arg_name != param_name { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-native-coercion-functions".into(),
        message: format!(
            "Prefer `{func_name}` directly over wrapping it in a function. \
             Use `.map({func_name})` instead of `.map(x => {func_name}(x))`."
        ),
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
    fn flags_map_arrow_number() {
        let d = run_on("arr.map(x => Number(x))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number"));
    }

    #[test]
    fn flags_map_arrow_string_parens() {
        let d = run_on("arr.map((s) => String(s))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("String"));
    }

    #[test]
    fn flags_map_arrow_boolean() {
        let d = run_on("arr.filter(v => Boolean(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Boolean"));
    }

    #[test]
    fn flags_block_body_return() {
        let d = run_on("arr.map(x => { return Number(x); })");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_usage() {
        assert!(run_on("arr.map(Number)").is_empty());
    }

    #[test]
    fn allows_different_param() {
        assert!(run_on("arr.map(x => Number(y))").is_empty());
    }

    #[test]
    fn allows_multiple_args() {
        assert!(run_on("arr.map(x => Number(x, 10))").is_empty());
    }

    #[test]
    fn flags_bigint_coercion() {
        let d = run_on("items.map(v => BigInt(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("BigInt"));
    }

    #[test]
    fn allows_non_coercion_function() {
        assert!(run_on("arr.map(x => parseInt(x))").is_empty());
    }
}
