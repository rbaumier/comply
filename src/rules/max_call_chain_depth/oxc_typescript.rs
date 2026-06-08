//! max-call-chain-depth OXC backend — flag deeply nested function calls like
//! f(g(h(i(x)))).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Argument;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn count_nested_calls_in_arg(arg: &Argument) -> usize {
    match arg {
        Argument::CallExpression(call) => {
            let mut max_depth = 1;
            for inner_arg in &call.arguments {
                let nested = count_nested_calls_in_arg(inner_arg);
                if nested > 0 {
                    max_depth = max_depth.max(1 + nested);
                }
            }
            max_depth
        }
        _ => 0,
    }
}

fn is_outermost_call<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let node_span = node.kind().span();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::CallExpression(parent_call) = parent.kind() {
            // If we're inside the arguments of a parent call, we're not outermost.
            for arg in &parent_call.arguments {
                let arg_span = arg.span();
                if node_span.start >= arg_span.start && node_span.end <= arg_span.end {
                    return false;
                }
            }
        }
        current_id = parent_id;
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if !is_outermost_call(node, semantic) {
            return;
        }

        let max = ctx.config.threshold("max-call-chain-depth", "max", ctx.lang);
        let mut depth = 1usize;
        for arg in &call.arguments {
            let nested = count_nested_calls_in_arg(arg);
            if nested > 0 {
                depth = depth.max(1 + nested);
            }
        }

        if depth > max {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Nested function calls have depth {depth} (max: {max}) \u{2014} extract intermediate variables."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_deeply_nested_calls() {
        // 5 levels: a(b(c(d(e(x)))))
        let src = "const x = a(b(c(d(e(1)))));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_nested_with_multiple_args() {
        // 5 levels: outer(process(transform(parse(read(file)))))
        let src = "const x = outer(process(transform(parse(read(file)))));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_shallow_nesting() {
        // 2 levels is fine
        let src = "const x = foo(bar(1));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_four_levels() {
        // max is 4, so exactly 4 is ok
        let src = "const x = a(b(c(d(1))));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn single_call_ok() {
        let src = "const x = foo(1);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn reports_only_once_per_chain() {
        let src = "const x = a(b(c(d(e(f(1))))));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn method_chain_not_flagged() {
        // Method chains are different — this should NOT be flagged
        let src = "const x = a.b().c().d().e().f();";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn sibling_calls_not_nested() {
        // These are siblings, not nested
        let src = "const x = combine(foo(1), bar(2), baz(3));";
        assert!(run_on(src).is_empty());
    }
}
