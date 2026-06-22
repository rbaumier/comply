//! xpath-injection oxc backend — flag dynamic XPath queries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression};
use std::sync::Arc;

/// XPath-specific method names — the MSXML DOM selection API. These do not
/// collide with SQL/ORM query builders, so a dynamic argument alone is enough.
const XPATH_SPECIFIC_METHODS: &[&str] = &["selectNodes", "selectSingleNode"];

/// Method names that XPath shares with unrelated APIs: `select`/`select1` are
/// the SQL/ORM column selector (Knex/TypeORM/Drizzle), and `evaluate` matches
/// AST/scope evaluators and `page.evaluate`. These only count as XPath when
/// provenance confirms it (XPath-looking receiver or XPath syntax in the query).
const XPATH_AMBIGUOUS_METHODS: &[&str] = &["select", "select1", "evaluate"];

/// Last identifier segment of a member expression's receiver, e.g. `scope` for
/// `state.scope.evaluate(...)` or `document` for `document.evaluate(...)`.
/// Returns `None` for receivers without a trailing name (calls, `this`, etc.).
fn receiver_name<'a>(object: &'a Expression<'a>) -> Option<&'a str> {
    match object {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

/// The receiver name signals an XPath processor (`document.evaluate`, the
/// `xpath` package, or any `*xpath*` variable), as opposed to a SQL query
/// builder or scope/AST evaluator.
fn is_xpath_receiver(object: &Expression) -> bool {
    let Some(name) = receiver_name(object) else { return false };
    name == "document" || name == "xpath" || name.to_ascii_lowercase().contains("xpath")
}

/// XPath syntax markers distinctive enough not to appear in SQL column
/// references or URL/path strings: descendant axis, attribute/axis predicates,
/// and node tests. A bare `/` is deliberately excluded — it collides with
/// URL/path literals passed to non-XPath `select`/`evaluate` calls.
const XPATH_SYNTAX_MARKERS: &[&str] = &["//", "[@", "::", "text()", "node()"];

/// Whether `text` (the static parts of a query string) contains XPath syntax.
/// A SQL selector like `table.primaryKey` has no markers; `//user[@id=` does.
fn contains_xpath_syntax(text: &str) -> bool {
    XPATH_SYNTAX_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Static (non-interpolated) text of the dynamic first argument: template
/// quasis for `` `//x[@a=${v}]` `` or string-literal operands of a `+` concat.
fn static_query_text(arg: &Argument) -> String {
    match arg {
        Argument::TemplateLiteral(tpl) => {
            tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<String>()
        }
        Argument::BinaryExpression(bin) => {
            let mut text = String::new();
            collect_concat_literals(&bin.left, &mut text);
            collect_concat_literals(&bin.right, &mut text);
            text
        }
        _ => String::new(),
    }
}

/// Appends string-literal text from a `+` concatenation tree, recursing into
/// nested additions so `'a' + x + 'b'` yields `ab`.
fn collect_concat_literals(expr: &Expression, out: &mut String) {
    match expr {
        Expression::StringLiteral(s) => out.push_str(s.value.as_str()),
        Expression::TemplateLiteral(tpl) => {
            for q in &tpl.quasis {
                out.push_str(q.value.raw.as_str());
            }
        }
        Expression::BinaryExpression(bin) if bin.operator == BinaryOperator::Addition => {
            collect_concat_literals(&bin.left, out);
            collect_concat_literals(&bin.right, out);
        }
        _ => {}
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Must cover every method `run` inspects, otherwise files containing
        // only e.g. `.select(` get pruned and the check never runs.
        Some(&["select", "evaluate", "selectNodes", "selectSingleNode"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression (e.g. xpath.select, doc.evaluate)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();
        let is_specific = XPATH_SPECIFIC_METHODS.contains(&method_name);
        let is_ambiguous = XPATH_AMBIGUOUS_METHODS.contains(&method_name);
        if !is_specific && !is_ambiguous {
            return;
        }

        // Must have at least one argument
        let Some(first_arg) = call.arguments.first() else { return };

        // Flag if first argument (XPath query) is dynamic
        let is_dynamic = match first_arg {
            Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
            Argument::BinaryExpression(bin) => bin.operator == BinaryOperator::Addition,
            Argument::Identifier(_)
            | Argument::StaticMemberExpression(_)
            | Argument::ComputedMemberExpression(_) => true,
            _ => false,
        };

        if !is_dynamic {
            return;
        }

        // `select`/`select1`/`evaluate` collide with SQL query builders and
        // scope/AST evaluators. Require XPath provenance: an XPath-looking
        // receiver, or XPath syntax in the static parts of the query string.
        if is_ambiguous
            && !is_xpath_receiver(&member.object)
            && !contains_xpath_syntax(&static_query_text(first_arg))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "XPath query with dynamic input — potential XPath injection.".into(),
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_dom_evaluate_with_dynamic_query() {
        assert_eq!(run("document.evaluate(query, doc)").len(), 1);
    }

    #[test]
    fn flags_xpath_package_evaluate() {
        assert_eq!(run("xpath.evaluate(expr, doc)").len(), 1);
    }

    #[test]
    fn flags_select_nodes_template() {
        assert_eq!(run("dom.selectNodes(`//user[@name='${name}']`)").len(), 1);
    }

    #[test]
    fn flags_select_single_node_concat() {
        assert_eq!(run("dom.selectSingleNode('//user[@id=' + id + ']')").len(), 1);
    }

    #[test]
    fn allows_static_dom_evaluate() {
        assert!(run("document.evaluate('//user', doc)").is_empty());
    }

    // Regression for #1763: `.evaluate()` on a non-XPath receiver (Svelte's
    // compiler-internal Scope) is an AST evaluation, not an XPath query.
    #[test]
    fn allows_scope_evaluate() {
        assert!(run("const evaluated = scope.evaluate(expression);").is_empty());
    }

    #[test]
    fn allows_nested_scope_evaluate() {
        assert!(run("const evaluated = state.scope.evaluate(node.expression);").is_empty());
    }

    #[test]
    fn allows_playwright_page_evaluate() {
        assert!(run("page.evaluate(selectorFn)").is_empty());
    }

    // Regression for #5367: Knex/ORM SQL query builders use `.select()` with a
    // dynamic column reference. No XPath receiver and no XPath syntax in the
    // query (`table.primaryKey`), so it must not be flagged.
    #[test]
    fn allows_knex_select_dynamic_column() {
        assert!(run("dbQuery.select(`${table}.${primaryKey}`)").is_empty());
    }

    #[test]
    fn allows_orm_select_concat_column() {
        assert!(run("qb.select(table + '.' + column)").is_empty());
    }

    // A bare `/` is not XPath syntax: a computed-column alias or path-like
    // string in an ORM select must not be flagged.
    #[test]
    fn allows_select_with_slash_non_xpath() {
        assert!(run("qb.select(`price/${unit}`)").is_empty());
    }

    // A genuine XPath select on the `xpath` package still fires.
    #[test]
    fn flags_xpath_package_select() {
        assert_eq!(run("xpath.select(`//user[@id='${id}']`, doc)").len(), 1);
    }

    // Provenance via XPath syntax in the query, even on an unknown receiver.
    #[test]
    fn flags_select_with_xpath_syntax_literal() {
        assert_eq!(run("processor.select(`//item[@id='${id}']`)").len(), 1);
    }

    #[test]
    fn flags_select1_with_xpath_syntax_concat() {
        assert_eq!(run("processor.select1('//user[@id=' + id + ']')").len(), 1);
    }
}
