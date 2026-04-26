//! max-call-chain-depth backend — flag deeply nested function calls like f(g(h(i(x)))).

use crate::diagnostic::{Diagnostic, Severity};

fn count_nested_calls(node: tree_sitter::Node) -> usize {
    if node.kind() != "call_expression" {
        return 0;
    }

    let mut max_depth = 1;

    // Check arguments for nested calls
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut cursor = args.walk();
        for arg in args.children(&mut cursor) {
            if arg.kind() == "call_expression" {
                let nested = count_nested_calls(arg);
                max_depth = max_depth.max(1 + nested);
            }
        }
    }

    max_depth
}

fn is_outermost_call(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "call_expression" => {
                // Check if we're in the arguments of the parent call
                if let Some(args) = parent.child_by_field_name("arguments")
                    && node.start_byte() >= args.start_byte()
                    && node.end_byte() <= args.end_byte()
                {
                    return false;
                }
            }
            "arguments" => {
                // We're inside arguments, so not outermost
                return false;
            }
            _ => {}
        }
        current = parent.parent();
    }
    true
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_outermost_call(node) {
        return;
    }

    let max = ctx.config.threshold("max-call-chain-depth", "max");
    let depth = count_nested_calls(node);

    if depth > max {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "max-call-chain-depth".into(),
            message: format!(
                "Nested function calls have depth {depth} (max: {max}) — extract intermediate variables."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
