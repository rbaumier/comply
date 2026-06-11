//! OxcCheck backend for js-index-maps — flag `.find()` / `.findIndex()` /
//! `.filter()` etc. inside loops.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression, NewExpression, Statement, UnaryOperator};
use oxc_semantic::ReferenceFlags;
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

        // Skip when the receiver is itself a property access (e.g. product.correspondences.find(...))
        // — relation fields are typically small and bounded; Map materialisation would be worse.
        if matches!(
            &member.object,
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        ) {
            return;
        }

        if !is_inside_loop(node, semantic) {
            return;
        }

        // The lookup is already O(1) when the callback predicate is a
        // `.has()` on a known `Set`/`Map` — the index the rule would suggest
        // building already exists.
        if callback_is_known_set_lookup(call, semantic) {
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

/// True when `call`'s callback predicate is a (possibly negated) `.has()`
/// lookup whose receiver is structurally known to be a `Set` or `Map`. Such a
/// lookup is O(1), so the flagged method does no O(n*m) scan.
fn callback_is_known_set_lookup<'a>(
    call: &CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(Argument::ArrowFunctionExpression(arrow)) = call.arguments.first() else {
        return false;
    };
    if !arrow.expression {
        return false;
    }
    let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
        return false;
    };

    let mut predicate = &stmt.expression;
    while let Expression::UnaryExpression(unary) = predicate {
        if unary.operator != UnaryOperator::LogicalNot {
            return false;
        }
        predicate = &unary.argument;
    }

    let Expression::CallExpression(lookup) = predicate else {
        return false;
    };
    let Expression::StaticMemberExpression(lookup_member) = &lookup.callee else {
        return false;
    };
    if lookup_member.property.name.as_str() != "has" {
        return false;
    }
    is_known_set_or_map(&lookup_member.object, semantic)
}

/// True when `expr` is structurally a `Set`/`Map`: a direct `new Set(...)` /
/// `new Map(...)`, or an identifier whose declaration initializer is one and
/// which is never reassigned.
fn is_known_set_or_map<'a>(expr: &Expression<'a>, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    match expr {
        Expression::NewExpression(new_expr) => is_set_or_map_constructor(new_expr),
        Expression::Identifier(id) => {
            let Some(ref_id) = id.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            if scoping
                .get_resolved_references(sym_id)
                .any(|reference| reference.flags().contains(ReferenceFlags::Write))
            {
                return false;
            }
            let AstKind::VariableDeclarator(decl) =
                semantic.nodes().kind(scoping.symbol_declaration(sym_id))
            else {
                return false;
            };
            matches!(&decl.init, Some(Expression::NewExpression(n)) if is_set_or_map_constructor(n))
        }
        _ => false,
    }
}

fn is_set_or_map_constructor(new_expr: &NewExpression<'_>) -> bool {
    matches!(
        &new_expr.callee,
        Expression::Identifier(id) if matches!(id.name.as_str(), "Set" | "Map")
    )
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn no_fp_on_relation_property_receiver() {
        // Regression for #757: product.correspondences is a bounded relation field.
        assert!(
            run(r#"
const fields = centrales.flatMap((centrale) => {
    const corr = product.correspondences.find((c) => c.centraleId === centrale.id) ?? null;
    return corr;
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_nested_member_chain() {
        // a.b.c is still a member expression — should not be flagged.
        assert!(
            run(r#"
items.forEach(item => {
    const x = a.b.c.find(v => v.id === item.id);
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_set_has_lookup_in_filter() {
        // Regression for #957: updatedGtins is a Set — `.has()` is already O(1).
        assert!(
            run(r#"
const updatedGtins = new Set(updatedRows.map((r) => r.gtin));
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_map_has_lookup_in_find_inside_loop() {
        assert!(
            run(r#"
const byId = new Map(items.map((i) => [i.id, i]));
for (const row of rows) {
    const known = candidates.find((c) => byId.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_direct_new_set_has_receiver() {
        assert!(
            run(r#"
const unknown = parsedRows
  .filter((r) => !new Set(updatedGtins).has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_includes_lookup_in_filter_chain() {
        // Plain-array `.includes()` is the genuine O(n*m) pattern.
        let diags = run(r#"
const updatedGtins = updatedRows.map((r) => r.gtin);
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.includes(r.gtin))
  .map((r) => r.gtin);
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_has_on_unknown_receiver() {
        // `updatedGtins` is not provably a Set/Map — keep flagging.
        let diags = run(r#"
const updatedGtins = getGtins();
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.has(r.gtin))
  .map((r) => r.gtin);
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_has_on_reassigned_receiver() {
        // The binding is reassigned after the Set declaration — no guarantee left.
        let diags = run(r#"
let updatedGtins = new Set(updatedRows.map((r) => r.gtin));
updatedGtins = getGtins();
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.has(r.gtin))
  .map((r) => r.gtin);
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_find_callback_over_plain_array_inside_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find((o) => candidates.find((c) => c.id === o.id));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_call_expression_receiver() {
        // getCategories() is a call result — unbounded, should still be flagged.
        let diags = run(r#"
items.map(item => {
    return getCategories().find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }
}
