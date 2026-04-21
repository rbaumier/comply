use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

struct AwaitStmt {
    binding: String,
    row: usize,
    col: usize,
}

/// Check if `word` appears in `text` as a standalone identifier (word boundary on both sides).
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

fn flush_run(run: &mut Vec<AwaitStmt>, diagnostics: &mut Vec<Diagnostic>, path: &std::path::Path) {
    if run.len() >= 2 {
        for stmt in run.iter() {
            diagnostics.push(Diagnostic {
                path: path.to_path_buf(),
                line: stmt.row + 1,
                column: stmt.col + 1,
                rule_id: "prefer-promise-all".into(),
                message: "Sequential `await` on independent results — use `Promise.all()` to run them in parallel.".into(),
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
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "statement_block" {
                return;
            }
            let mut run: Vec<AwaitStmt> = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "lexical_declaration" {
                    flush_run(&mut run, &mut diagnostics, ctx.path);
                    continue;
                }
                let decl = match child.named_child(0) {
                    Some(d) if d.kind() == "variable_declarator" => d,
                    _ => {
                        flush_run(&mut run, &mut diagnostics, ctx.path);
                        continue;
                    }
                };
                let name_node = match decl.child_by_field_name("name") {
                    Some(n) => n,
                    None => {
                        flush_run(&mut run, &mut diagnostics, ctx.path);
                        continue;
                    }
                };
                let val_node = match decl.child_by_field_name("value") {
                    Some(v) => v,
                    None => {
                        flush_run(&mut run, &mut diagnostics, ctx.path);
                        continue;
                    }
                };
                if val_node.kind() != "await_expression" {
                    flush_run(&mut run, &mut diagnostics, ctx.path);
                    continue;
                }
                let binding = name_node.utf8_text(source).unwrap_or("").to_owned();
                let call_text = val_node.utf8_text(source).unwrap_or("").to_owned();
                let pos = child.start_position();
                let dependent = run.iter().any(|s| contains_word(&call_text, &s.binding));
                if dependent {
                    flush_run(&mut run, &mut diagnostics, ctx.path);
                }
                run.push(AwaitStmt {
                    binding,
                    row: pos.row,
                    col: pos.column,
                });
            }
            flush_run(&mut run, &mut diagnostics, ctx.path);
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_two_independent_awaits() {
        let src = r#"
async function f() {
  const a = await fetchUser();
  const b = await fetchPosts();
}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_dependent_await() {
        let src = r#"
async function f() {
  const a = await fetchUser();
  const b = await fetchPosts(a.id);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_await() {
        assert!(run("async function f() { const a = await fetch('/api'); }").is_empty());
    }

    #[test]
    fn allows_promise_all_already() {
        let src = r#"
async function f() {
  const [a, b] = await Promise.all([fetchUser(), fetchPosts()]);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_three_independent_awaits() {
        let src = r#"
async function f() {
  const a = await fetchA();
  const b = await fetchB();
  const c = await fetchC();
}
"#;
        assert!(run(src).len() >= 2);
    }
}
