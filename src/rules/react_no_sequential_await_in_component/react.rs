//! react-no-sequential-await-in-component backend — detect two or more
//! independent `await` expressions in a row inside an async React component
//! body.
//!
//! Scope: only fires when the enclosing function is both `async` AND has a
//! PascalCase name (React's component naming convention). This distinguishes
//! it from the generic `prefer-promise-all` rule, which fires on any function.
//!
//! Independence heuristic: two adjacent `const x = await ...` declarations
//! are considered independent unless the second's initializer textually
//! references the first's binding. Same logic as `prefer-promise-all`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

struct AwaitStmt {
    /// Individual identifiers introduced by this declaration.
    bindings: Vec<String>,
    row: usize,
    col: usize,
}

/// Extract individual identifier names from a binding pattern node.
/// Handles simple identifiers, object patterns (`{ id, name }`,
/// `{ id: userId }`), and array patterns (`[first, second]`).
fn extract_bindings(node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    match node.kind() {
        "object_pattern" => {
            let mut out = Vec::new();
            let count = node.named_child_count();
            for i in 0..count {
                let child = node.named_child(i).unwrap();
                match child.kind() {
                    "shorthand_property_identifier_pattern"
                    | "shorthand_property_identifier" => {
                        if let Ok(t) = child.utf8_text(source) {
                            out.push(t.to_owned());
                        }
                    }
                    "pair_pattern" => {
                        if let Some(val) = child.child_by_field_name("value") {
                            out.extend(extract_bindings(val, source));
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        "array_pattern" => {
            let mut out = Vec::new();
            let count = node.named_child_count();
            for i in 0..count {
                let child = node.named_child(i).unwrap();
                out.extend(extract_bindings(child, source));
            }
            out
        }
        _ => {
            let text = node.utf8_text(source).unwrap_or("").to_owned();
            if text.is_empty() { vec![] } else { vec![text] }
        }
    }
}

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let wbytes = word.as_bytes();
    let wlen = word.len();
    if wlen == 0 {
        return false;
    }
    let mut i = 0;
    while i + wlen <= bytes.len() {
        if &bytes[i..i + wlen] == wbytes {
            let before_ok =
                i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let after_ok = i + wlen >= bytes.len()
                || !(bytes[i + wlen].is_ascii_alphanumeric() || bytes[i + wlen] == b'_');
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_async_source(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.utf8_text(source)
        .map(|t| t.trim_start().starts_with("async "))
        .unwrap_or(false)
}

/// True when `block` is the body of an async function whose name is
/// PascalCase (React component convention).
fn is_async_component_body(block: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = block.parent() else {
        return false;
    };
    match parent.kind() {
        "function_declaration" => {
            if !is_async_source(parent, source) {
                return false;
            }
            let Some(name_node) = parent.child_by_field_name("name") else {
                return false;
            };
            let Ok(name) = name_node.utf8_text(source) else {
                return false;
            };
            starts_with_uppercase(name)
        }
        "arrow_function" | "function_expression" | "function" => {
            if !is_async_source(parent, source) {
                return false;
            }
            // Walk up: arrow assigned to a PascalCase const?
            let mut cur = parent.parent();
            while let Some(n) = cur {
                if n.kind() == "variable_declarator" {
                    let Some(name_node) = n.child_by_field_name("name") else {
                        return false;
                    };
                    let Ok(name) = name_node.utf8_text(source) else {
                        return false;
                    };
                    return starts_with_uppercase(name);
                }
                cur = n.parent();
            }
            false
        }
        _ => false,
    }
}

fn flush_run(run: &mut Vec<AwaitStmt>, diagnostics: &mut Vec<Diagnostic>, path: &std::path::Path) {
    if run.len() >= 2 {
        for stmt in run.iter() {
            diagnostics.push(Diagnostic {
                path: path.to_path_buf().into(),
                line: stmt.row + 1,
                column: stmt.col + 1,
                rule_id: "react-no-sequential-await-in-component".into(),
                message: "Sequential `await` inside an async React component \
                          serialises fetches. Combine independent awaits with \
                          `Promise.all([...])` to parallelise them."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
    run.clear();
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["statement_block"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        if !is_async_component_body(node, source) {
            return;
        }

        let mut run: Vec<AwaitStmt> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "lexical_declaration" {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            }
            let Some(decl) = child.named_child(0) else {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            };
            if decl.kind() != "variable_declarator" {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            }
            let Some(name_node) = decl.child_by_field_name("name") else {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            };
            let Some(val_node) = decl.child_by_field_name("value") else {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            };
            if val_node.kind() != "await_expression" {
                flush_run(&mut run, diagnostics, ctx.path);
                continue;
            }
            let bindings = extract_bindings(name_node, source);
            let call_text = val_node.utf8_text(source).unwrap_or("").to_owned();
            let pos = child.start_position();
            let dependent = run.iter().any(|s| {
                s.bindings.iter().any(|b| contains_word(&call_text, b))
            });
            if dependent {
                flush_run(&mut run, diagnostics, ctx.path);
            }
            run.push(AwaitStmt {
                bindings,
                row: pos.row,
                col: pos.column,
            });
        }
        flush_run(&mut run, diagnostics, ctx.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_two_independent_awaits_in_component() {
        let src = r#"
export default async function Page() {
    const user = await getUser();
    const posts = await getPosts();
    return <div>{user.name}</div>;
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_three_independent_awaits_in_component() {
        let src = r#"
async function Dashboard() {
    const a = await getA();
    const b = await getB();
    const c = await getC();
    return <div />;
}
"#;
        assert!(run_on(src).len() >= 2);
    }

    #[test]
    fn allows_dependent_awaits() {
        let src = r#"
async function Page() {
    const user = await getUser();
    const posts = await getPosts(user.id);
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dependent_destructured_object() {
        let src = r#"
async function Page() {
    const { id } = await getUser();
    const posts = await getPosts(id);
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dependent_renamed_destructuring() {
        let src = r#"
async function Page() {
    const { id: userId } = await getUser();
    const posts = await getPosts(userId);
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dependent_array_destructuring() {
        let src = r#"
async function Page() {
    const [first] = await getItems();
    const details = await getDetails(first);
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_promise_all_already() {
        let src = r#"
async function Page() {
    const [user, posts] = await Promise.all([getUser(), getPosts()]);
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_component_async_function() {
        // lowercase `loadData` — not a React component.
        let src = r#"
async function loadData() {
    const a = await getA();
    const b = await getB();
    return [a, b];
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_sync_component() {
        let src = r#"
function Page() {
    return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_arrow_component() {
        let src = r#"
export const Page = async () => {
    const a = await getA();
    const b = await getB();
    return <div />;
};
"#;
        assert_eq!(run_on(src).len(), 2);
    }
}
