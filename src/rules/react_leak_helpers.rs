//! Shared helpers for the `react-no-leaked-*` family of rules.
//!
//! Every rule in the family detects the same shape: a resource
//! creation call (`addEventListener`, `setInterval`, `setTimeout`,
//! `fetch`, `new ResizeObserver(...)`, …) inside a `useEffect`
//! callback whose body does NOT register a corresponding cleanup
//! (return a function that releases the resource).
//!
//! The helpers expose:
//! - `find_use_effect_callbacks(semantic)` — iterator over the
//!   `useEffect(() => { ... })` callback bodies in the file.
//! - `body_has_cleanup_keyword(body_text, keywords)` — heuristic:
//!   does the textual body contain a `return` statement whose payload
//!   mentions any of the cleanup keywords?

use oxc_ast::ast::{Expression, FunctionBody, Statement};

/// Return `Some(body_text)` for every `useEffect(() => { ... })` /
/// `useEffect(function() { ... })` callback whose body we can inspect.
/// Body text spans cover the function body (including the braces).
pub fn use_effect_bodies<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &'a str,
) -> Vec<(&'a FunctionBody<'a>, &'a str)> {
    let mut out: Vec<(&'a FunctionBody<'a>, &'a str)> = Vec::new();
    for node in semantic.nodes().iter() {
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        if !callee_is_use_effect(&call.callee) {
            continue;
        }
        let Some(first_arg) = call.arguments.first() else {
            continue;
        };
        let body: &FunctionBody<'a> = match first_arg {
            oxc_ast::ast::Argument::ArrowFunctionExpression(a) => &a.body,
            oxc_ast::ast::Argument::FunctionExpression(f) => match f.body.as_deref() {
                Some(b) => b,
                None => continue,
            },
            _ => continue,
        };
        let span_start = body.span.start as usize;
        let span_end = body.span.end as usize;
        if span_end > source.len() {
            continue;
        }
        out.push((body, &source[span_start..span_end]));
    }
    out
}

fn callee_is_use_effect(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => matches!(
            id.name.as_str(),
            "useEffect" | "useLayoutEffect" | "useInsertionEffect"
        ),
        Expression::StaticMemberExpression(member) => matches!(
            member.property.name.as_str(),
            "useEffect" | "useLayoutEffect" | "useInsertionEffect"
        ),
        _ => false,
    }
}

/// True if the body contains a `return` whose argument (textually)
/// mentions any of the cleanup keywords. Heuristic — no full type
/// resolution, but catches the documented React cleanup pattern:
///
/// ```ts
/// useEffect(() => {
///   target.addEventListener(...);
///   return () => target.removeEventListener(...);
/// }, []);
/// ```
pub fn body_returns_cleanup(body: &FunctionBody, source: &str, keywords: &[&str]) -> bool {
    for stmt in &body.statements {
        if let Statement::ReturnStatement(ret) = stmt
            && let Some(arg) = &ret.argument
        {
            let start = arg.span().start as usize;
            let end = arg.span().end as usize;
            if end <= source.len() {
                let arg_text = &source[start..end];
                if keywords.iter().any(|kw| arg_text.contains(kw)) {
                    return true;
                }
            }
        }
    }
    false
}

use oxc_span::GetSpan;
