//! zod-string-min-1-required backend — flag `z.string()` calls that are
//! not chained with at least one method that constrains length, format,
//! optionality, or transforms the input. The check walks up from the
//! `z.string()` call expression and inspects whether the parent is a
//! `member_expression` calling one of the accepted continuations.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
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
}
