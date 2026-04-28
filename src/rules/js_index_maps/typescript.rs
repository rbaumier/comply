//! Flags `.find()` / `.findIndex()` / `.filter()` inside loop constructs.

use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

const ITERATOR_METHODS: &[&str] = &["forEach", "map", "flatMap", "reduce", "some", "every"];

const LOOKUP_METHODS: &[&str] = &["find", "findIndex", "filter", "includes", "indexOf"];

fn is_inside_loop(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        if LOOP_KINDS.contains(&a.kind()) {
            return true;
        }
        // Named function/class/method boundaries — hoisted definitions
        // don't necessarily execute per iteration.
        if matches!(
            a.kind(),
            "function_declaration" | "class_declaration" | "method_definition"
        ) {
            return false;
        }
        // .forEach() / .map() etc. count as loops.
        if a.kind() == "call_expression" {
            if let Some(callee) = a.child_by_field_name("function") {
                if callee.kind() == "member_expression" {
                    if let Some(prop) = callee.child_by_field_name("property") {
                        if let Ok(method) = prop.utf8_text(source) {
                            if ITERATOR_METHODS.contains(&method) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        if a.kind() == "program" {
            break;
        }
        ancestor = a.parent();
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(method) = prop.utf8_text(source) else { return };
    if !LOOKUP_METHODS.contains(&method) {
        return;
    }

    if !is_inside_loop(node, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`.{method}()` inside a loop is O(n*m) — build a `Map` or `Set` for O(1) lookups."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_find_in_for_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find(o => o.id === item.id);
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".find()"));
    }

    #[test]
    fn flags_find_in_for_statement() {
        let diags = run(r#"
for (let i = 0; i < items.length; i++) {
    const m = arr.findIndex(x => x.id === items[i].id);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_filter_in_while() {
        let diags = run(r#"
while (hasMore) {
    const filtered = items.filter(i => i.active);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_foreach() {
        let diags = run(r#"
items.forEach(item => {
    const match = others.find(o => o.id === item.id);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_map() {
        let diags = run(r#"
const result = items.map(item => {
    return categories.find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_find_outside_loop() {
        assert!(run(r#"
const user = users.find(u => u.id === targetId);
"#).is_empty());
    }

    #[test]
    fn allows_map_without_find() {
        assert!(run(r#"
const names = items.map(i => i.name);
"#).is_empty());
    }

    #[test]
    fn allows_find_on_non_loop_call() {
        assert!(run(r#"
function process() {
    const item = arr.find(x => x.id === id);
    return item;
}
"#).is_empty());
    }

    #[test]
    fn allows_find_in_named_function_inside_loop() {
        assert!(run(r#"
items.forEach(item => {
    function helper() { return others.find(o => o.id === id); }
    return helper;
});
"#).is_empty());
    }
}
