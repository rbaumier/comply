//! zod-string-min-1-required: flag `z.string()` calls without a length/format/optionality continuation.
//! Skipped in test files: fixtures use `z.string()` as a stand-in stub, never `.parse()`d at runtime.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e.", ".e2e-spec."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case("tests") || name.eq_ignore_ascii_case("e2e")
    })
}

/// Variable-name substrings that mark a schema as a response/wire-contract
/// (server-emitted) shape rather than a user-input schema. The server controls
/// the wire format, so `z.string()` fields need not be non-empty.
const RESPONSE_SCHEMA_MARKERS: &[&str] = &[
    "Response", "Output", "Result", "Reply", "Wire",
    "Dto", "DTO", "Error", "Problem",
];

/// True when `z.string()` lives inside a variable whose name contains a
/// response/wire-contract marker (e.g. `ProblemSchema`, `FooResponseSchema`).
fn is_inside_response_schema(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::VariableDeclarator(decl) = ancestor.kind() {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id else {
                return false;
            };
            return RESPONSE_SCHEMA_MARKERS.iter().any(|m| id.name.contains(m));
        }
    }
    false
}

const VALID_CONTINUATIONS: &[&str] = &[
    "min",
    "max",
    "email",
    "url",
    "uuid",
    "regex",
    "length",
    "startsWith",
    "endsWith",
    "optional",
    "nullable",
    "nullish",
    "trim",
    "toLowerCase",
    "toUpperCase",
];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// Check if this call expression is the object of a parent member expression
/// with a valid continuation method. We do this by checking if this call is
/// wrapped in a `z.string().min(1)` style chain via the source text around
/// the call span.
fn is_chained_with_valid_continuation(call_end: u32, source: &str) -> bool {
    let rest = &source[call_end as usize..];
    let trimmed = rest.trim_start();
    if let Some(after_dot) = trimmed.strip_prefix('.') {
        let method: String = after_dot
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        return VALID_CONTINUATIONS.contains(&method.as_str());
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if is_test_file(ctx.path) {
            return;
        }

        if is_inside_response_schema(node, semantic) {
            return;
        }

        let Some(name) = callee_name(&call.callee) else { return };
        if name != "z.string" {
            return;
        }

        // Check if this z.string() is chained with a valid continuation.
        if is_chained_with_valid_continuation(call.span.end, ctx.source) {
            return;
        }

        // z.string() passed directly as an argument to a function: the wrapper
        // may apply constraints internally (e.g. refineNoControlChars adds .min(1)).
        if matches!(semantic.nodes().parent_node(node.id()).kind(), AstKind::CallExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `z.string()` accepts empty strings \u{2014} add `.min(1)` or a format constraint.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, path)
    }

    #[test]
    fn flags_bare_string() {
        assert_eq!(run("const s = z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_min() {
        assert!(run("z.string().min(1)").is_empty());
    }

    #[test]
    fn allows_email() {
        assert!(run("z.string().email()").is_empty());
    }

    #[test]
    fn allows_optional() {
        assert!(run("z.string().optional()").is_empty());
    }

    #[test]
    fn no_fp_when_passed_to_wrapper_function() {
        // Regression for issue #428: z.string() passed to a helper that applies .min(1) internally.
        assert!(run("refineNoControlChars(z.string(), 'label')").is_empty());
        assert!(run("refineNoControlChars(z.string(), fieldLabel)").is_empty());
        assert!(run("const s = refineNoControlChars(z.string(), 'x')").is_empty());
    }

    #[test]
    fn still_flags_bare_string_in_object() {
        // z.string() inside an object literal is not passed to a wrapper — still flagged.
        assert_eq!(run("z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_bare_string_in_test_file() {
        // Regression for issue #119: `z.string()` in test fixtures is a
        // typed stand-in that is never `.parse()`d at runtime.
        let code = "const schema = z.object({ sort: z.string() });";
        assert!(run_at(code, "src/foo.test.ts").is_empty());
        assert!(run_at(code, "src/foo.spec.ts").is_empty());
        assert!(run_at(code, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(code, "e2e/foo.ts").is_empty());
        assert!(run_at(code, "tests/foo.ts").is_empty());
        assert!(run_at(code, "src/foo.e2e-spec.ts").is_empty());
        assert!(run_at(code, "src/foo_test.ts").is_empty());
    }

    #[test]
    fn no_fp_on_response_wire_contract_schema() {
        // Regression for issue #513: RFC 7807 Problem JSON response schema —
        // z.string() fields in response/wire-contract schemas must not require
        // .min(1) because the server may legitimately emit empty strings.
        let rfc7807 = r#"
            export const ProblemSchema = z.object({
                type: z.string(),
                title: z.string(),
                status: z.number(),
                detail: z.string(),
                instance: z.string(),
            });
        "#;
        assert!(run(rfc7807).is_empty());

        // Other common response-schema naming conventions.
        assert!(run("const FooResponseSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooResponse = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooOutputSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const UserDto = z.object({ name: z.string() });").is_empty());
        assert!(run("const ApiErrorSchema = z.object({ message: z.string() });").is_empty());
        assert!(run("const SearchResult = z.object({ label: z.string() });").is_empty());
    }

    #[test]
    fn still_flags_bare_string_in_input_schema() {
        // Ensure response-schema exemption does not apply to input schemas.
        assert_eq!(run("const loginSchema = z.object({ username: z.string() });").len(), 1);
        assert_eq!(run("const CreateUserInput = z.object({ name: z.string() });").len(), 1);
    }
}
