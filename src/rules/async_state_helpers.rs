//! Shared async-state vocabulary — the words a hand-rolled request lifecycle
//! is spelled with, plus the type walks that find them.
//!
//! Two rules read a request lifecycle out of TypeScript source:
//! `no-homemade-async-state-union` (literal unions and `{ data, loading, error }`
//! objects) and `react-no-request-state-mirror` (`useState("idle")` beside a
//! TanStack Query call). They share one vocabulary so they can never disagree
//! on what counts as a request phase: the lists live in
//! `[rules.no-homemade-async-state-union]` of `src/config/defaults.toml` and
//! both rules read them through this module.

use crate::rules::backend::{AstKind, CheckCtx};
use oxc_ast::ast::{CallExpression, Expression, TSLiteral, TSType};

/// The `defaults.toml` section that owns the shared vocabulary. Naming one
/// owner keeps the two rules on the same word list; a per-rule copy would let
/// them drift the first time someone edits one of them.
const VOCABULARY_OWNER: &str = "no-homemade-async-state-union";

/// The module whose own `status` union is the thing being duplicated.
pub const QUERY_MODULE: &str = "@tanstack/react-query";

/// Every string literal that can name a phase of an in-flight request.
#[must_use]
pub fn async_literals(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .required_string_list(VOCABULARY_OWNER, "async_literals", ctx.lang)
}

/// The subset of [`async_literals`] that can only describe a request.
/// `pending` also names an order and `failed` a payment, so a union built from
/// those alone is domain state; one of these words is what makes it async.
#[must_use]
pub fn async_only_literals(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .required_string_list(VOCABULARY_OWNER, "async_only_literals", ctx.lang)
}

/// Field names a hand-rolled async state object is built from.
#[must_use]
pub fn async_fields(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .required_string_list(VOCABULARY_OWNER, "async_fields", ctx.lang)
}

/// The subset of [`async_fields`] that has to carry a boolean before the
/// object counts as a request-state mirror. Without one, `{ data, error }` is
/// a `Result` in disguise, not a state machine.
#[must_use]
pub fn async_only_fields(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .required_string_list(VOCABULARY_OWNER, "async_only_fields", ctx.lang)
}

/// The failure channel of [`async_fields`], also required. A state machine has
/// somewhere to put the failure; `{ data, isLoading }` with nowhere to report
/// one is a presentational component's props, fed by a query it does not own.
#[must_use]
pub fn async_error_fields(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .required_string_list(VOCABULARY_OWNER, "async_error_fields", ctx.lang)
}

/// Share of an object's named fields that must be async fields before the
/// object counts as a state machine.
#[must_use]
pub fn min_async_field_ratio(ctx: &CheckCtx) -> f64 {
    ctx.config
        .float(VOCABULARY_OWNER, "min_async_field_ratio", ctx.lang)
}

/// Case-insensitive membership — `"LOADING"` and `"loading"` name one phase.
#[must_use]
pub fn matches(value: &str, words: &[String]) -> bool {
    words.iter().any(|word| word.eq_ignore_ascii_case(value))
}

/// Collect every string-literal member of `ty` into `out`. The parser keeps
/// parentheses and nested unions as their own nodes, so `("idle" | "loading") |
/// null` has to be walked to reach both literals.
pub fn collect_string_literals<'a>(ty: &'a TSType<'a>, out: &mut Vec<&'a str>) {
    match ty {
        TSType::TSUnionType(union) => {
            for member in &union.types {
                collect_string_literals(member, out);
            }
        }
        TSType::TSParenthesizedType(paren) => collect_string_literals(&paren.type_annotation, out),
        TSType::TSLiteralType(literal) => {
            if let TSLiteral::StringLiteral(text) = &literal.literal {
                out.push(text.value.as_str());
            }
        }
        _ => {}
    }
}

/// True when the file imports `@tanstack/react-query`. A substring match on
/// the source would also accept the module name quoted in a comment, so this
/// reads the import declarations.
#[must_use]
pub fn imports_query_module(semantic: &oxc_semantic::Semantic) -> bool {
    semantic.nodes().iter().any(|node| {
        matches!(node.kind(), AstKind::ImportDeclaration(import)
            if import.source.value.as_str() == QUERY_MODULE)
    })
}

/// True for `useState(...)` and `React.useState(...)`.
#[must_use]
pub fn is_use_state_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "useState",
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "useState"
                && matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "React")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    /// Collect the string literals of the first type alias in `src`.
    fn literals_of_first_alias(src: &str) -> Vec<String> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, src, SourceType::ts()).parse();
        let mut found = Vec::new();
        for statement in &parsed.program.body {
            if let oxc_ast::ast::Statement::TSTypeAliasDeclaration(alias) = statement {
                let mut literals = Vec::new();
                collect_string_literals(&alias.type_annotation, &mut literals);
                found = literals.iter().map(|value| (*value).to_string()).collect();
                break;
            }
        }
        found
    }

    #[test]
    fn collects_a_flat_union() {
        assert_eq!(
            literals_of_first_alias(r#"type S = "idle" | "loading";"#),
            ["idle", "loading"]
        );
    }

    #[test]
    fn collects_through_parentheses_and_nesting() {
        assert_eq!(
            literals_of_first_alias(r#"type S = ("idle" | "loading") | null;"#),
            ["idle", "loading"]
        );
    }

    #[test]
    fn ignores_non_string_members() {
        assert!(literals_of_first_alias("type S = 200 | 404;").is_empty());
    }

    #[test]
    fn matches_ignores_case() {
        let words = vec!["loading".to_string()];
        assert!(matches("LOADING", &words));
        assert!(!matches("loaded", &words));
    }
}
