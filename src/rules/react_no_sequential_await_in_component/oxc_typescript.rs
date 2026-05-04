//! OxcCheck backend for react-no-sequential-await-in-component.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

struct AwaitStmt {
    bindings: Vec<String>,
    offset: u32,
}

/// Extract individual identifier names from a binding pattern.
fn extract_bindings(pattern: &BindingPattern) -> Vec<String> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => {
            vec![id.name.to_string()]
        }
        BindingPattern::ObjectPattern(obj) => {
            let mut out = Vec::new();
            for prop in &obj.properties {
                out.extend(extract_bindings(&prop.value));
            }
            if let Some(rest) = &obj.rest {
                out.extend(extract_bindings(&rest.argument));
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

fn flush_run(
    run: &mut Vec<AwaitStmt>,
    source: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if run.len() >= 2 {
        for stmt in run.iter() {
            let (line, column) = byte_offset_to_line_col(source, stmt.offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
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

fn check_body(
    body: &oxc_ast::ast::FunctionBody,
    source: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut run: Vec<AwaitStmt> = Vec::new();

    for stmt in &body.statements {
        let Statement::VariableDeclaration(decl) = stmt else {
            flush_run(&mut run, source, ctx, diagnostics);
            continue;
        };
        if decl.declarations.len() != 1 {
            flush_run(&mut run, source, ctx, diagnostics);
            continue;
        }
        let declarator = &decl.declarations[0];
        let Some(init) = &declarator.init else {
            flush_run(&mut run, source, ctx, diagnostics);
            continue;
        };
        let Expression::AwaitExpression(await_expr) = init else {
            flush_run(&mut run, source, ctx, diagnostics);
            continue;
        };

        let bindings = extract_bindings(&declarator.id);
        let call_text = &source[await_expr.span.start as usize..await_expr.span.end as usize];

        let dependent = run
            .iter()
            .any(|s| s.bindings.iter().any(|b| contains_word(call_text, b)));
        if dependent {
            flush_run(&mut run, source, ctx, diagnostics);
        }
        run.push(AwaitStmt {
            bindings,
            offset: stmt.span().start,
        });
    }
    flush_run(&mut run, source, ctx, diagnostics);
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FunctionBody]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FunctionBody(body) = node.kind() else {
            return;
        };

        // Check if this body belongs to an async PascalCase component
        let nodes = semantic.nodes();
        let Some(parent) = nodes.ancestors(node.id()).nth(1) else {
            return;
        };

        let is_component = match parent.kind() {
            AstKind::Function(func) => {
                if !func.r#async {
                    return;
                }
                func.id
                    .as_ref()
                    .is_some_and(|id| starts_with_uppercase(id.name.as_str()))
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if !arrow.r#async {
                    return;
                }
                // Walk up to find variable declarator
                nodes.ancestors(parent.id()).nth(1).is_some_and(|gp| {
                    if let AstKind::VariableDeclarator(decl) = gp.kind() {
                        if let BindingPattern::BindingIdentifier(id) = &decl.id {
                            return starts_with_uppercase(id.name.as_str());
                        }
                    }
                    false
                })
            }
            _ => false,
        };

        if !is_component {
            return;
        }

        check_body(body, ctx.source, ctx, diagnostics);
    }
}
