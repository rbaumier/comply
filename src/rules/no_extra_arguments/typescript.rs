//! no-extra-arguments backend — flag calls with more args than params.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashMap;

struct FunctionInfo {
    param_count: usize,
    has_rest: bool,
}

fn has_rest_in_subtree(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    if kind == "rest_pattern" || kind == "rest_parameter" || kind == "spread_element" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_rest_in_subtree(child) {
            return true;
        }
    }
    false
}

fn count_params(params_node: tree_sitter::Node) -> (usize, bool) {
    let mut count = 0;
    let mut has_rest = false;
    let mut cursor = params_node.walk();

    for child in params_node.children(&mut cursor) {
        let kind = child.kind();
        match kind {
            "identifier" | "required_parameter" | "optional_parameter" | "assignment_pattern" => {
                // Check if this param contains a rest pattern
                if has_rest_in_subtree(child) {
                    has_rest = true;
                } else {
                    count += 1;
                }
            }
            "rest_pattern" | "rest_parameter" | "spread_element" => {
                has_rest = true;
            }
            "formal_parameters" => {
                // Nested params (shouldn't happen but be safe)
                let (c, r) = count_params(child);
                count += c;
                has_rest = has_rest || r;
            }
            _ => {
                // For any unrecognized node, check for rest patterns inside
                if has_rest_in_subtree(child) {
                    has_rest = true;
                }
            }
        }
    }

    (count, has_rest)
}

fn get_function_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            let name = node.child_by_field_name("name")?;
            name.utf8_text(source).ok()
        }
        "variable_declarator" => {
            let name = node.child_by_field_name("name")?;
            name.utf8_text(source).ok()
        }
        "method_definition" | "function" => {
            let name = node.child_by_field_name("name")?;
            name.utf8_text(source).ok()
        }
        _ => None,
    }
}

fn collect_functions<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    functions: &mut HashMap<String, FunctionInfo>,
) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(name) = get_function_name(node, source)
                && let Some(params) = node.child_by_field_name("parameters")
            {
                let (count, has_rest) = count_params(params);
                functions.insert(
                    name.to_string(),
                    FunctionInfo {
                        param_count: count,
                        has_rest,
                    },
                );
            }
        }
        "variable_declarator" => {
            if let Some(name) = get_function_name(node, source)
                && let Some(value) = node.child_by_field_name("value")
                && (value.kind() == "arrow_function" || value.kind() == "function")
                && let Some(params) = value.child_by_field_name("parameters")
            {
                let (count, has_rest) = count_params(params);
                functions.insert(
                    name.to_string(),
                    FunctionInfo {
                        param_count: count,
                        has_rest,
                    },
                );
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, functions);
    }
}

fn count_args(args_node: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = args_node.walk();

    for child in args_node.children(&mut cursor) {
        // Skip punctuation
        if child.kind() != "," && child.kind() != "(" && child.kind() != ")" {
            count += 1;
        }
    }

    count
}

fn check_calls<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    functions: &HashMap<String, FunctionInfo>,
    diagnostics: &mut Vec<Diagnostic>,
    path: &std::path::Path,
) {
    if node.kind() == "call_expression"
        && let Some(func) = node.child_by_field_name("function")
        && func.kind() == "identifier"
        && let Ok(name) = func.utf8_text(source)
        && let Some(info) = functions.get(name)
        && !info.has_rest
        && let Some(args) = node.child_by_field_name("arguments")
    {
        let arg_count = count_args(args);
        if arg_count > info.param_count {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::from(path),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-extra-arguments".into(),
                message: format!(
                    "Function `{name}` expects {} argument(s) but got {arg_count}.",
                    info.param_count
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_calls(child, source, functions, diagnostics, path);
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let mut functions = HashMap::new();
    collect_functions(node, source, &mut functions);
    check_calls(node, source, &functions, diagnostics, ctx.path);
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
    fn flags_extra_argument() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2, 3);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_arrow_function_extra_args() {
        let src = r#"
            const bar = (x) => x * 2;
            bar(1, 2);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_correct_args() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2);
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fewer_args() {
        let src = r#"
            function foo(a, b) {}
            foo(1);
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_rest_params() {
        let src = r#"
            function foo(a, ...rest) {}
            foo(1, 2, 3, 4, 5);
        "#;
        let diags = run_on(src);
        if !diags.is_empty() {
            // Debug: print what we got
            for d in &diags {
                eprintln!("Diagnostic: {}", d.message);
            }
        }
        assert!(diags.is_empty(), "Expected no diagnostics for rest params");
    }

    #[test]
    fn allows_unknown_function() {
        let src = "externalFn(1, 2, 3, 4, 5);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_multiple_extra_calls() {
        let src = r#"
            function foo(a) {}
            foo(1, 2);
            foo(1, 2, 3);
        "#;
        assert_eq!(run_on(src).len(), 2);
    }
}
