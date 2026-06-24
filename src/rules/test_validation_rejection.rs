//! Shared detection of schema-validation rejection assertions.
//!
//! A bare `.toThrow()` on `expect(() => schema.parse(x))` asserts "invalid
//! input is rejected" — the thrown error type (`ZodError`, `ValidationError`,
//! …) is guaranteed by the library's design and pinning it adds only boilerplate
//! noise. Both `require-to-throw-message` (issue #993) and `test-check-exception`
//! (issue #1338) exempt this shape, so the detection lives here once rather than
//! duplicated across the two `.toThrow()` rules.

use crate::rules::backend::AstKind;
use oxc_ast::ast::{Argument, Expression};
use oxc_span::Span;

/// Schema-validation methods whose rejection is the test contract. Covers
/// zod/valibot (`parse`/`parseAsync`), yup (`validate`/`validateSync`/`cast`),
/// and joi (`validate`/`attempt`).
const VALIDATION_METHODS: &[&str] = &[
    "parse",
    "parseAsync",
    "validate",
    "validateSync",
    "cast",
    "attempt",
];

/// Returns true when the `.toThrow()` receiver is an `expect(...)` call (the
/// `expect(...)` may be wrapped in member chains like `.rejects`/`.resolves`)
/// whose first argument is a callback invoking a schema-validation method.
///
/// `object` is the member-expression object the `.toThrow` property hangs off,
/// i.e. the `expect(...)` (or `expect(...).rejects`) sub-expression.
pub fn is_validation_rejection_subject<'a>(
    object: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = object;
    let expect_call = loop {
        match current {
            Expression::StaticMemberExpression(member) => current = &member.object,
            Expression::CallExpression(call) => break call,
            _ => return false,
        }
    };
    let Expression::Identifier(callee) = &expect_call.callee else {
        return false;
    };
    if callee.name.as_str() != "expect" {
        return false;
    }

    let callback_span = match expect_call.arguments.first() {
        Some(Argument::ArrowFunctionExpression(arrow)) => arrow.span,
        Some(Argument::FunctionExpression(func)) => func.span,
        _ => return false,
    };

    semantic.nodes().iter().any(|node| {
        let AstKind::CallExpression(inner) = node.kind() else {
            return false;
        };
        if !contains_span(callback_span, inner.span) {
            return false;
        }
        callee_property_name(&inner.callee).is_some_and(|name| VALIDATION_METHODS.contains(&name))
    })
}

fn contains_span(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// Property name of a member-expression callee (`x.parse(...)` or
/// `x["parse"](...)`); `None` for non-member callees.
fn callee_property_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        Expression::ComputedMemberExpression(member) => match &member.expression {
            Expression::StringLiteral(lit) => Some(lit.value.as_str()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    /// Parse `src`, locate the `.toThrow()` member call, and run the detection
    /// on its receiver — the same `&member.object` the rules pass in.
    fn detect(src: &str) -> bool {
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if member.property.name.as_str() == "toThrow" {
                return is_validation_rejection_subject(&member.object, &semantic);
            }
        }
        panic!("no `.toThrow()` member call found in source");
    }

    #[test]
    fn detects_every_validation_method() {
        for method in VALIDATION_METHODS {
            assert!(
                detect(&format!("expect(() => schema.{method}(x)).toThrow();")),
                "{method} should be detected as a validation rejection",
            );
        }
    }

    #[test]
    fn detects_block_bodied_callback() {
        assert!(detect("expect(() => { schema.parse(x); }).toThrow();"));
    }

    #[test]
    fn detects_callback_behind_rejects_chain() {
        assert!(detect(
            "expect(async () => { await schema.parseAsync(x); }).rejects.toThrow();",
        ));
    }

    #[test]
    fn detects_computed_member_parse() {
        assert!(detect(r#"expect(() => schema["parse"](x)).toThrow();"#));
    }

    #[test]
    fn rejects_non_validation_method() {
        assert!(!detect("expect(() => service.compute()).toThrow();"));
    }

    #[test]
    fn rejects_bare_identifier_subject() {
        // `expect(insert)` passes a thenable, not a validation callback.
        assert!(!detect("expect(insert).rejects.toThrow();"));
    }

    #[test]
    fn rejects_free_function_call_in_callback() {
        // A bare call with no schema-method receiver is not a validation parse.
        assert!(!detect("expect(() => doStuff()).toThrow();"));
    }

    #[test]
    fn rejects_non_expect_receiver() {
        assert!(!detect("notExpect(() => schema.parse(x)).toThrow();"));
    }
}
