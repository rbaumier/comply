//! OxcCheck backend for js-index-maps — flag `.find()` / `.findIndex()` /
//! `.filter()` etc. inside loops.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const LOOKUP_METHODS: &[&str] = &["find", "findIndex", "filter", "includes", "indexOf"];
const ITERATOR_METHODS: &[&str] = &["forEach", "map", "flatMap", "reduce", "some", "every"];

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

        // Match `.find(...)`, `.findIndex(...)`, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !LOOKUP_METHODS.contains(&method) {
            return;
        }

        if !is_inside_loop(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` inside a loop is O(n*m) — build a `Map` or `Set` for O(1) lookups."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,

            // Named function/class/method boundaries — hoisted definitions
            // don't necessarily execute per iteration.
            AstKind::Function(f) if f.id.is_some() => return false,
            AstKind::Class(_) => return false,

            // .forEach() / .map() etc. count as loops.
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee
                    && ITERATOR_METHODS.contains(&member.property.name.as_str()) {
                        return true;
                    }
            }

            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
        assert!(
            run(r#"
const user = users.find(u => u.id === targetId);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_map_without_find() {
        assert!(
            run(r#"
const names = items.map(i => i.name);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_on_non_loop_call() {
        assert!(
            run(r#"
function process() {
    const item = arr.find(x => x.id === id);
    return item;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_in_named_function_inside_loop() {
        assert!(
            run(r#"
items.forEach(item => {
    function helper() { return others.find(o => o.id === id); }
    return helper;
});
"#)
            .is_empty()
        );
    }
}
