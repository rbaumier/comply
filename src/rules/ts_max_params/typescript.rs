//! ts-max-params backend — flag functions with more than 3 parameters.
//!
//! Counts parameters in function declarations, arrow functions, function
//! expressions, and method definitions. Skips `this` parameters (TS-specific).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_func = matches!(
        node.kind(),
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "abstract_method_signature"
    );
    if !is_func {
        return;
    }

    let max_params = ctx.config.threshold("ts-max-params", "max", ctx.lang);

    let Some(params) = node.child_by_field_name("parameters") else {
        return;
    };

    let mut count = 0usize;
    let mut cursor = params.walk();
    for child in params.named_children(&mut cursor) {
        // Skip TS `this` parameter (e.g. `function f(this: Foo, a: number)`)
        if child.kind() == "required_parameter" || child.kind() == "optional_parameter" {
            // Check if the parameter name is `this`
            if let Some(name_node) = child.child_by_field_name("pattern")
                && name_node.utf8_text(source).unwrap_or("") == "this" {
                    continue;
                }
        }
        // Count all named children in formal_parameters that are actual params
        match child.kind() {
            "required_parameter" | "optional_parameter" | "rest_pattern"
            | "assignment_pattern" | "identifier" | "object_pattern"
            | "array_pattern" => {
                count += 1;
            }
            _ => {}
        }
    }

    if count > max_params {
        // Get the function name if available
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-max-params".into(),
            message: format!(
                "Function `{name}` has {count} parameters (maximum allowed is {max_params})."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_three_params() {
        assert!(run_on("function f(a: number, b: string, c: boolean) {}").is_empty());
    }

    #[test]
    fn flags_four_params() {
        let d = run_on("function f(a: number, b: string, c: boolean, d: number) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("4 parameters"));
    }

    #[test]
    fn flags_arrow_function() {
        let d = run_on("const f = (a: number, b: string, c: boolean, d: number) => {};");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_this_parameter() {
        // `this` is a TS-specific parameter that shouldn't count
        assert!(run_on("function f(this: Foo, a: number, b: string, c: boolean) {}").is_empty());
    }
}
