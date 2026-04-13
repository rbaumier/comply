//! no-identical-functions backend — flag functions with identical bodies.

use crate::diagnostic::{Diagnostic, Severity};

/// Normalize a body's text: collapse whitespace per line, drop empties.
fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only process at the program (root) level to collect all functions once.
    if node.kind() != "program" {
        return;
    }

    // Collect all function-like declarations and their normalized bodies.
    let mut functions: Vec<(String, usize, String)> = Vec::new(); // (name, line, normalized_body)

    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        collect_functions(child, source, &mut functions);
    }

    // Compare pairs.
    for i in 1..functions.len() {
        for j in 0..i {
            if functions[i].2 == functions[j].2 {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: functions[i].1,
                    column: 1,
                    rule_id: "no-identical-functions".into(),
                    message: format!(
                        "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                        functions[i].0,
                        functions[j].0,
                        functions[j].1,
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
    }
}

fn collect_functions<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    functions: &mut Vec<(String, usize, String)>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some((name, line, body)) = extract_function_info(node, source) {
                let normalized = normalize_body(&body);
                // Only flag functions with >3 lines to avoid trivial matches.
                if body.lines().count() > 3 {
                    functions.push((name, line, normalized));
                }
            }
        }
        "lexical_declaration" => {
            // const foo = (...) => { ... }  or  const foo = function(...) { ... }
            let count = node.named_child_count();
            for i in 0..count {
                let Some(declarator) = node.named_child(i) else { continue };
                if declarator.kind() != "variable_declarator" {
                    continue;
                }
                let Some(name_node) = declarator.child_by_field_name("name") else { continue };
                let Ok(name) = name_node.utf8_text(source) else { continue };
                let Some(value) = declarator.child_by_field_name("value") else { continue };

                let body_node = match value.kind() {
                    "arrow_function" | "function" => value.child_by_field_name("body"),
                    _ => None,
                };
                if let Some(body_n) = body_node
                    && let Ok(body_text) = body_n.utf8_text(source) {
                        let normalized = normalize_body(body_text);
                        if body_text.lines().count() > 3 {
                            let line = name_node.start_position().row + 1;
                            functions.push((name.to_string(), line, normalized));
                        }
                    }
            }
        }
        "export_statement" => {
            // Recurse into exported declarations.
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, functions);
                }
            }
        }
        _ => {}
    }
}

fn extract_function_info(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<(String, usize, String)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let body_node = node.child_by_field_name("body")?;
    let body = body_node.utf8_text(source).ok()?;
    let line = name_node.start_position().row + 1;
    Some((name.to_string(), line, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_identical_functions() {
        let src = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_different_functions() {
        let src = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x - 1;
    const b = a / 2;
    console.log(b);
    return b;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_identical_bodies() {
        let src = r#"
function foo() {
    return 1;
}

function bar() {
    return 1;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
