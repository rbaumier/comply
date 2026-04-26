use crate::diagnostic::{Diagnostic, Severity};

fn count_yield_stars(node: &tree_sitter::Node<'_>, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    let mut stack: Vec<tree_sitter::Node<'_>> = vec![*node];
    while let Some(n) = stack.pop() {
        if n.kind() == "yield_expression" {
            let text = n.utf8_text(source).unwrap_or("");
            if text.starts_with("yield*") || text.starts_with("yield *") {
                count += 1;
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.utf8_text(source).unwrap_or("") != "Result.gen" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    // Find the generator function arg
    let mut cursor = args.walk();
    let mut generator: Option<tree_sitter::Node<'_>> = None;
    for child in args.children(&mut cursor) {
        if matches!(child.kind(), "generator_function" | "generator_function_declaration" | "function_expression") {
            generator = Some(child);
            break;
        }
    }
    let Some(gen_fn) = generator else { return; };
    let yields = count_yield_stars(&gen_fn, source);
    if yields != 1 {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Result.gen wrapping a single yield* — use .map()/.andThen() instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_single_yield_gen() {
        let src = "const r = Result.gen(function* () { const v = yield* getUser(); return v; });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_multi_yield_gen() {
        let src = "const r = Result.gen(function* () { const u = yield* getUser(); const o = yield* getOrders(u); return o; });";
        assert!(run(src).is_empty());
    }
}
