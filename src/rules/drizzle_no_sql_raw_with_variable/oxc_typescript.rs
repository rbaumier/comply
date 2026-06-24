//! drizzle-no-sql-raw-with-variable — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, IdentifierReference, VariableDeclarationKind};
use std::sync::Arc;

pub struct Check;

/// Returns true when every template expression is wrapped in SQL double-quote
/// identifier syntax — `"${expr}"`. Such calls are safe DDL-identifier
/// patterns; bare `${expr}` interpolations remain flagged.
fn all_expressions_double_quoted(tpl: &oxc_ast::ast::TemplateLiteral) -> bool {
    for (i, _) in tpl.expressions.iter().enumerate() {
        let before = tpl.quasis[i].value.raw.as_str();
        let after = tpl.quasis[i + 1].value.raw.as_str();
        if !before.ends_with('"') || !after.starts_with('"') {
            return false;
        }
    }
    true
}

/// Returns true when `ident` resolves to a `const` binding whose initializer is
/// a string literal — i.e. the value is a compile-time constant with the exact
/// safety profile of inlining the literal into `sql.raw("…")`, which the rule
/// already exempts.
///
/// SECURITY: this is the *only* provenance the rule trusts, and deliberately so.
/// It exempts solely the case where the AST proves the reachable value is a
/// fixed string literal — `const x = "idx_synthetic_stale_trgm"` then
/// `sql.raw(x)`. It does NOT attempt to prove that a value read from a query
/// (e.g. `row.name` from a `pg_class` system-catalog read, issue #344) is safe:
/// that needs inter-statement taint analysis an AST walk cannot do soundly, and
/// any heuristic loose enough to catch it would also wave through genuinely
/// user-derived data. Those sites keep their per-site `comply-ignore`.
///
/// `const` is required, not merely "declared with a string-literal initializer":
/// a `let`/`var` binding can be reassigned to attacker-controlled data later in
/// scope, so its initializer does not pin the value reaching `sql.raw`. oxc's
/// scope resolution picks the binding actually in scope, so shadowing is handled.
///
/// Deliberately narrow: only a `StringLiteral` initializer is trusted. A
/// no-expression template-literal-bound const (`const x = `idx``) or
/// `const x = "a" as const` is also a fixed string but stays flagged — over-
/// flagging a provably-safe value is the safe direction, and the agreed scope of
/// issue #344 is the string-literal case only. Widen only on a documented need.
fn is_const_string_literal(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    // For a variable binding, the symbol's declaration node *is* the
    // `VariableDeclarator` (not an ancestor), so match it directly. A
    // destructuring binding (`const { x } = …`) resolves to a declarator whose
    // `.init` is the RHS expression, not a `StringLiteral`, so the init gate
    // below rejects it.
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();

    let AstKind::VariableDeclarator(declarator) = nodes.kind(decl_node_id) else {
        return false;
    };
    if !matches!(declarator.init, Some(Expression::StringLiteral(_))) {
        return false;
    }

    // The declarator's parent `VariableDeclaration` must carry `const`.
    matches!(
        nodes.kind(nodes.parent_id(decl_node_id)),
        AstKind::VariableDeclaration(decl) if decl.kind == VariableDeclarationKind::Const
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sql.raw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `sql.raw`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "sql" || member.property.name.as_str() != "raw" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        // String literal → safe.
        if matches!(first_arg, Argument::StringLiteral(_)) {
            return;
        }
        // Template literal → safe when no expressions (static string) or all
        // expressions are wrapped in SQL double-quote identifier syntax.
        if let Argument::TemplateLiteral(tpl) = first_arg
            && all_expressions_double_quoted(tpl)
        {
            return;
        }
        // `const`-bound string literal → safe (equivalent to inlining the
        // literal, which the StringLiteral arm above already exempts). See
        // `is_const_string_literal` for why this is the only provenance trusted.
        if let Argument::Identifier(ident) = first_arg
            && is_const_string_literal(ident, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`sql.raw()` with a non-literal argument is a SQL injection vector — use `sql` tagged templates with parameterized values instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_variable_argument() {
        assert_eq!(run("sql.raw(userInput)").len(), 1);
    }

    #[test]
    fn flags_unquoted_template_substitution() {
        assert_eq!(run("sql.raw(`SELECT * FROM ${tableName}`)").len(), 1);
    }

    #[test]
    fn flags_mixed_quoted_and_unquoted() {
        assert_eq!(
            run(r#"sql.raw(`"${id}" WHERE col = ${value}`)"#).len(),
            1
        );
    }

    #[test]
    fn allows_string_literal() {
        assert!(run(r#"sql.raw("SELECT 1")"#).is_empty());
    }

    #[test]
    fn allows_static_template_literal() {
        assert!(run("sql.raw(`SELECT 1`)").is_empty());
    }

    /// Regression for issue #344: sql.raw with a DDL identifier from pg_class
    /// must not be flagged when the identifier is properly double-quoted.
    #[test]
    fn allows_double_quoted_identifier_in_template() {
        assert!(run(r#"sql.raw(`DROP INDEX IF EXISTS "${row.name}"`)"#).is_empty());
    }

    #[test]
    fn allows_multiple_double_quoted_identifiers() {
        assert!(run(r#"sql.raw(`ALTER TABLE "${schema}"."${table}" ADD COLUMN id int`)"#).is_empty());
    }

    /// Regression for the reopened issue #344: a `const` bound to a string
    /// literal (the `syntheticIndex` DDL-identifier case) has the exact safety
    /// profile of inlining the literal, so it must not be flagged.
    #[test]
    fn allows_const_string_literal_variable() {
        assert!(
            run(r#"const syntheticIndex = "idx_synthetic_stale_trgm"; sql.raw(syntheticIndex);"#)
                .is_empty()
        );
    }

    /// Scope resolution must pick the binding actually in scope: an inner
    /// `const` shadow bound to user input at the call site must be flagged even
    /// when an outer same-named `const` holds a safe literal.
    #[test]
    fn flags_inner_const_shadow_bound_to_user_input() {
        assert_eq!(
            run(r#"const name = "idx"; { const name = req.body.x; sql.raw(name); }"#).len(),
            1
        );
    }

    /// Symmetric to the above: an inner `const` literal shadow at the call site
    /// is exempt even when an outer same-named binding holds a tainted value —
    /// the exemption keys on the resolved in-scope binding, not the textual name.
    #[test]
    fn allows_inner_const_literal_shadow() {
        assert!(
            run(r#"const name = req.body.x; { const name = "idx"; sql.raw(name); }"#).is_empty()
        );
    }

    /// A destructured `const` binding's initializer is the RHS expression, not a
    /// string literal, so it stays flagged — taint cannot be smuggled through a
    /// destructuring pattern.
    #[test]
    fn flags_destructured_const() {
        assert_eq!(run(r#"const { name } = req.body; sql.raw(name);"#).len(), 1);
    }

    /// A `let` binding can be reassigned to attacker-controlled data after the
    /// declaration, so its string-literal initializer does not pin the value
    /// reaching `sql.raw` — it must stay flagged.
    #[test]
    fn flags_let_string_literal_variable() {
        assert_eq!(
            run(r#"let name = "idx"; name = req.body.x; sql.raw(name);"#).len(),
            1
        );
    }

    /// A `var` binding is mutable for the same reason as `let` — stay flagged.
    #[test]
    fn flags_var_string_literal_variable() {
        assert_eq!(run(r#"var name = "idx"; sql.raw(name);"#).len(), 1);
    }

    /// A `const` whose initializer is NOT a string literal (here a function
    /// call) carries an unknown, potentially user-derived value — stay flagged.
    #[test]
    fn flags_const_non_string_initializer() {
        assert_eq!(run(r#"const name = getName(); sql.raw(name);"#).len(), 1);
    }

    /// The `pg_class.relname` provenance case (issue #344) is deliberately NOT
    /// exempted: a value read from a query is not a const string literal, so an
    /// AST walk cannot prove it safe. It must still be flagged (the site keeps a
    /// per-site `comply-ignore`). This pins that the fix did not widen too far.
    #[test]
    fn flags_member_expression_from_query_row() {
        assert_eq!(run("sql.raw(row.name)").len(), 1);
    }

    /// A genuinely user-derived variable must still be flagged — the core true
    /// positive the security rule exists to catch.
    #[test]
    fn flags_user_input_variable() {
        assert_eq!(
            run(r#"const name = req.body.indexName; sql.raw(name);"#).len(),
            1
        );
    }
}
