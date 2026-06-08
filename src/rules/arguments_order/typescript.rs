//! arguments-order: detects call sites where argument names suggest wrong order.
//!
//! Compares parameter names from function signatures with argument names at
//! call sites. Flags when argument names match parameter names but in wrong
//! positions, suggesting a potential argument swap.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // 1. Collect function declarations with their parameter names
    let mut signatures: HashMap<String, Vec<String>> = HashMap::new();
    collect_function_signatures(node, source, &mut signatures);

    // 2. Merge exported function params from ImportIndex
    let index = ctx.project.import_index();
    for imp in index.get_imports(ctx.path) {
        let Some(src_path) = &imp.source_path else { continue; };
        for export in index.get_exports(src_path) {
            if export.name == imp.imported_name && !export.params.is_empty() {
                signatures.insert(imp.local_name.clone(), export.params.clone());
            }
        }
    }

    if signatures.is_empty() { return; }

    // 3. Check all call sites
    check_calls(node, source, ctx.path, &signatures, diagnostics);
}

fn collect_function_signatures(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut HashMap<String, Vec<String>>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
        {
            let params = extract_param_names(node, source);
            if !params.is_empty() {
                out.insert(name.to_string(), params);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn extract_param_names(func: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let Some(params) = func.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = params.walk();
    for child in params.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                if let Ok(name) = child.utf8_text(source) {
                    result.push(name.to_string());
                }
            }
            "required_parameter" | "optional_parameter" => {
                if let Some(pattern) = child.child_by_field_name("pattern")
                    && pattern.kind() == "identifier"
                    && let Ok(name) = pattern.utf8_text(source)
                {
                    result.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    result
}

fn check_calls(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    path: &std::path::Path,
    signatures: &HashMap<String, Vec<String>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(callee) = node.child_by_field_name("function")
            && callee.kind() == "identifier"
            && let Ok(name) = callee.utf8_text(source)
            && let Some(params) = signatures.get(name)
        {
            check_call_args(node, source, path, name, params, diagnostics);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn check_call_args(
    call: tree_sitter::Node<'_>,
    source: &[u8],
    path: &std::path::Path,
    func_name: &str,
    params: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(args_node) = call.child_by_field_name("arguments") else {
        return;
    };

    let mut args: Vec<Option<String>> = Vec::new();
    let mut cursor = args_node.walk();
    for child in args_node.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            args.push(child.utf8_text(source).ok().map(String::from));
        } else {
            args.push(None);
        }
    }

    // Check for swapped arguments
    if let Some(swap) = find_likely_swap(params, &args) {
        let pos = call.start_position();
        diagnostics.push(Diagnostic {
            path: path.to_path_buf().into(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "arguments-order".into(),
            message: format!(
                "Argument order may be wrong in `{}()`: '{}' and '{}' appear swapped.",
                func_name, swap.0, swap.1
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns (arg1, arg2) if they appear to be swapped based on parameter names.
fn find_likely_swap(params: &[String], args: &[Option<String>]) -> Option<(String, String)> {
    // Need at least 2 params and args to detect a swap
    if params.len() < 2 || args.len() < 2 {
        return None;
    }

    // Look for cases where arg[i] matches param[j] and arg[j] matches param[i]
    for i in 0..params.len().min(args.len()) {
        for j in (i + 1)..params.len().min(args.len()) {
            let Some(arg_i) = &args[i] else {
                continue;
            };
            let Some(arg_j) = &args[j] else {
                continue;
            };

            // Check if args are swapped relative to params
            // arg[i] matches param[j] AND arg[j] matches param[i]
            if names_match(arg_i, &params[j]) && names_match(arg_j, &params[i]) {
                return Some((arg_i.clone(), arg_j.clone()));
            }
        }
    }
    None
}

/// Check if an argument name matches a parameter name (case-insensitive, ignoring common prefixes).
fn names_match(arg: &str, param: &str) -> bool {
    let arg_norm = normalize_name(arg);
    let param_norm = normalize_name(param);
    arg_norm == param_norm
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase().trim_start_matches('_').to_string()
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
    fn flags_swapped_args() {
        let code = r#"
            function createUser(name, email) { }
            createUser(email, name);
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_correct_order() {
        let code = r#"
            function createUser(name, email) { }
            createUser(name, email);
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_different_names() {
        let code = r#"
            function createUser(name, email) { }
            createUser(foo, bar);
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_expressions() {
        let code = r#"
            function createUser(name, email) { }
            createUser(getName(), getEmail());
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_with_underscore_prefix() {
        let code = r#"
            function foo(_a, _b) { }
            foo(b, a);
        "#;
        assert_eq!(run(code).len(), 1);
    }
}
