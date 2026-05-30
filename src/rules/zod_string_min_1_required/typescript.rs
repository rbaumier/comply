//! zod-string-min-1-required: flag `z.string()` calls without a length/format/optionality continuation.
//! Skipped in test files: fixtures use `z.string()` as a stand-in stub, never `.parse()`d at runtime.

use crate::diagnostic::{Diagnostic, Severity};

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

/// Variable-name substrings that mark a schema as a response/wire-contract shape.
const RESPONSE_SCHEMA_MARKERS: &[&str] = &[
    "Response", "Output", "Result", "Reply", "Wire",
    "Dto", "DTO", "Error", "Problem",
];

fn enclosed_in_response_schema(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source) {
                    return RESPONSE_SCHEMA_MARKERS.iter().any(|m| name.contains(m));
                }
            }
            return false;
        }
        cur = parent;
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

crate::ast_check! { prefilter = ["z.string"] => |node, source, ctx, diagnostics|
    if is_test_file(ctx.path) {
        return;
    }

    if enclosed_in_response_schema(node, source) {
        return;
    }

    // Match `z.string()` itself.
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.string" { return; }

    // Walk up; if the parent chain ever hits a member_expression whose
    // `object` is this `z.string()` call AND whose property is one of the
    // accepted continuation methods, accept.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "member_expression" {
            // Confirm this node is the `object` of the member_expression.
            if let Some(obj) = parent.child_by_field_name("object")
                && obj.id() == cur.id()
            {
                let Some(prop) = parent.child_by_field_name("property") else { break };
                let Ok(prop_text) = prop.utf8_text(source) else { break };
                if VALID_CONTINUATIONS.iter().any(|c| *c == prop_text) {
                    return;
                }
                break;
            }
        }
        // Allow walking past wrapping `call_expression` nodes (rare here).
        if parent.kind() == "call_expression" {
            cur = parent;
            continue;
        }
        // z.string() (or its chain) is a direct argument to a function call:
        // the wrapper may apply constraints internally (e.g. refineNoControlChars).
        if parent.kind() == "arguments" {
            return;
        }
        break;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Bare `z.string()` accepts empty strings — add `.min(1)` or a format constraint.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, path)
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
        // z.string() inside an object literal is not passed directly to a wrapper — still flagged.
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
        // Regression for issue #513.
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

        assert!(run("const FooResponseSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooResponse = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooOutputSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const UserDto = z.object({ name: z.string() });").is_empty());
        assert!(run("const ApiErrorSchema = z.object({ message: z.string() });").is_empty());
        assert!(run("const SearchResult = z.object({ label: z.string() });").is_empty());
    }

    #[test]
    fn still_flags_bare_string_in_input_schema() {
        assert_eq!(run("const loginSchema = z.object({ username: z.string() });").len(), 1);
        assert_eq!(run("const CreateUserInput = z.object({ name: z.string() });").len(), 1);
    }
}
