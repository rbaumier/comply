//! prefer-top-level-await backend — flag async IIFE and `async main(); main()` patterns.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is an async IIFE: `(async () => { ... })()`
fn is_async_iife(node: tree_sitter::Node, source: &[u8]) -> bool {
    // call_expression whose callee is a parenthesized_expression wrapping
    // an arrow_function or function_expression with `async` keyword
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "parenthesized_expression" {
        return false;
    }
    // The inner expression should be an arrow_function or function (expression)
    let mut cursor = callee.walk();
    for child in callee.children(&mut cursor) {
        match child.kind() {
            "arrow_function" | "function" | "function_expression" => {
                // Check if it has the `async` keyword
                let Ok(text) = child.utf8_text(source) else { return false };
                return text.starts_with("async ");
            }
            _ => {}
        }
    }
    false
}

/// Check if node is a top-level `async function name(…)` declaration.
/// Returns the function name if so.
fn async_func_decl_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "function_declaration" {
        return None;
    }
    // Must be at the program level (parent is program or export_statement)
    let parent = node.parent()?;
    if parent.kind() != "program" && parent.kind() != "export_statement" {
        return None;
    }
    // Check async keyword
    let Ok(text) = node.utf8_text(source) else { return None };
    if !text.starts_with("async ") {
        return None;
    }
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Skip CJS files
    let path_str = ctx.path.to_string_lossy();
    if path_str.ends_with(".cjs") {
        return;
    }

    // Pattern 1: async IIFE at the top level
    if is_async_iife(node, source) {
        // Must be top-level (parent is program or expression_statement whose parent is program)
        let is_top_level = node.parent().is_some_and(|p| {
            p.kind() == "program"
                || (p.kind() == "expression_statement"
                    && p.parent().is_some_and(|pp| pp.kind() == "program"))
        });
        if is_top_level {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-top-level-await".into(),
                message: "Prefer top-level await over an async IIFE.".into(),
                severity: Severity::Warning,
            });
            return;
        }
    }

    // Pattern 2: top-level call to an async function defined at top level
    if node.kind() == "call_expression" {
        let Some(callee) = node.child_by_field_name("function") else { return };
        let Ok(callee_text) = callee.utf8_text(source) else { return };

        // Must be top-level call
        let is_top_level = node.parent().is_some_and(|p| {
            p.kind() == "expression_statement"
                && p.parent().is_some_and(|pp| pp.kind() == "program")
        });
        if !is_top_level {
            return;
        }

        // The callee might be `name` or `name().then(…)` — handle the
        // simple `name()` case by scanning siblings for async function decls.
        // Also handle `name().then(…)` — the callee here would be a
        // member_expression `name().then`.
        let func_name = if callee.kind() == "identifier" {
            callee_text
        } else if callee.kind() == "member_expression" {
            // name().then — extract name from `name()` call
            let Some(obj) = callee.child_by_field_name("object") else { return };
            if obj.kind() != "call_expression" { return; }
            let Some(inner_callee) = obj.child_by_field_name("function") else { return };
            if inner_callee.kind() != "identifier" { return; }
            let Ok(name) = inner_callee.utf8_text(source) else { return };
            name
        } else {
            return;
        };

        // Walk the program to find async function declarations with this name
        let Some(program) = node.parent().and_then(|p| p.parent()) else { return };
        let mut pcursor = program.walk();
        for top_child in program.children(&mut pcursor) {
            let decl = if top_child.kind() == "function_declaration" {
                top_child
            } else if top_child.kind() == "export_statement" {
                let mut ec = top_child.walk();
                match top_child.children(&mut ec).find(|c| c.kind() == "function_declaration") {
                    Some(fd) => fd,
                    None => continue,
                }
            } else {
                continue;
            };
            if let Some(name) = async_func_decl_name(decl, source)
                && name == func_name {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "prefer-top-level-await".into(),
                        message: format!(
                            "Prefer top-level await over calling async function `{func_name}()`."
                        ),
                        severity: Severity::Warning,
                    });
                    return;
                }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_async_arrow_iife() {
        let d = run_on("(async () => { await fetch('/api'); })();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-top-level-await");
    }

    #[test]
    fn flags_async_function_iife() {
        let d = run_on("(async function() { await fetch('/api'); })();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_async_function_then_call() {
        let src = "async function main() {\n  await fetch('/api');\n}\nmain();";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("main"));
    }

    #[test]
    fn flags_async_function_with_then() {
        let src = "async function bootstrap() {\n  await init();\n}\nbootstrap().then(() => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_regular_function() {
        let src = "function main() {\n  console.log('hello');\n}\nmain();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_await_directly() {
        assert!(run_on("const data = await fetch('/api');").is_empty());
    }
}
