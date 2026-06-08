//! prefer-promise-all OXC backend — flag sequential awaits on independent results.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, Statement, VariableDeclarationKind};
use oxc_span::GetSpan;
use std::sync::Arc;

struct AwaitStmt {
    bindings: Vec<String>,
    span_start: u32,
}

/// Extract individual identifier names from a binding pattern.
fn extract_bindings(pattern: &oxc_ast::ast::BindingPattern) -> Vec<String> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => {
            vec![id.name.as_str().to_string()]
        }
        BindingPattern::ObjectPattern(obj) => {
            let mut out = Vec::new();
            for prop in &obj.properties {
                out.extend(extract_bindings(&prop.value));
            }
            out
        }
        BindingPattern::ArrayPattern(arr) => {
            let mut out = Vec::new();
            for elem in arr.elements.iter().flatten() {
                out.extend(extract_bindings(elem));
            }
            out
        }
        BindingPattern::AssignmentPattern(assign) => extract_bindings(&assign.left),
    }
}

/// Check if `word` appears in `text` as a standalone identifier.
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

fn flush_run(run: &mut Vec<AwaitStmt>, diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx) {
    if run.len() >= 2 {
        for stmt in run.iter() {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, stmt.span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Sequential `await` on independent results — use `Promise.all()` to run them in parallel.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
    run.clear();
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BlockStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BlockStatement(block) = node.kind() else {
            return;
        };

        let mut run: Vec<AwaitStmt> = Vec::new();

        for stmt in &block.body {
            let Statement::VariableDeclaration(decl) = stmt else {
                flush_run(&mut run, diagnostics, ctx);
                continue;
            };

            // Must be a lexical declaration (const/let).
            if decl.kind == VariableDeclarationKind::Var {
                flush_run(&mut run, diagnostics, ctx);
                continue;
            }

            let Some(declarator) = decl.declarations.first() else {
                flush_run(&mut run, diagnostics, ctx);
                continue;
            };

            let Some(init) = &declarator.init else {
                flush_run(&mut run, diagnostics, ctx);
                continue;
            };

            let Expression::AwaitExpression(_) = init else {
                flush_run(&mut run, diagnostics, ctx);
                continue;
            };

            let bindings = extract_bindings(&declarator.id);
            let call_text =
                &ctx.source[init.span().start as usize..init.span().end as usize];

            let dependent = run
                .iter()
                .any(|s| s.bindings.iter().any(|b| contains_word(call_text, b)));
            if dependent {
                flush_run(&mut run, diagnostics, ctx);
            }

            run.push(AwaitStmt {
                bindings,
                span_start: stmt.span().start,
            });
        }
        flush_run(&mut run, diagnostics, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
    fn allows_dependent_destructured_object() {
        let src = r#"
async function load() {
  const { id } = await getUser();
  const posts = await getPosts(id);
}
"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_dependent_renamed_destructuring() {
        let src = r#"
async function load() {
  const { id: userId } = await getUser();
  const posts = await getPosts(userId);
}
"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_dependent_array_destructuring() {
        let src = r#"
async function load() {
  const [first] = await getItems();
  const details = await getDetails(first);
}
"#;
        assert!(run(src).is_empty());
    }
}
