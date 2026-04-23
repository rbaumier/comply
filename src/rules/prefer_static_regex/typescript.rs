use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for regex literals
    if node.kind() != "regex" { return; }

    // Check if inside a function
    let mut current = node.parent();
    let mut inside_function = false;

    while let Some(parent) = current {
        match parent.kind() {
            "function_declaration" | "function_expression" | "arrow_function"
            | "method_definition" | "generator_function" | "generator_function_declaration" => {
                inside_function = true;
                break;
            }
            "program" | "class_body" => break,
            _ => {}
        }
        current = parent.parent();
    }

    if !inside_function { return; }

    // Skip if regex uses variables (new RegExp with template)
    // We only flag literal /.../ inside functions

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-static-regex".into(),
        message: "Regex literal inside function is recompiled on each call. Hoist to module scope.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_regex_in_function() {
        assert_eq!(run("function f() { return /abc/.test(s); }").len(), 1);
        assert_eq!(run("const f = () => /abc/.test(s)").len(), 1);
    }

    #[test]
    fn flags_regex_in_method() {
        let code = "class C { m() { return /abc/.test(s); } }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        assert!(run("const RE = /abc/;").is_empty());
        assert!(run("const RE = /abc/g;").is_empty());
    }

    #[test]
    fn allows_class_property_regex() {
        assert!(run("class C { re = /abc/; }").is_empty());
    }
}
