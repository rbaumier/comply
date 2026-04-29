//! no-unchecked-json-parse backend — flag unwrapped `JSON.parse(...)` calls.
//!
//! Why: `JSON.parse()` types its return as `any`, so the parsed payload
//! silently poisons every downstream call site. Wrapping the call with a
//! Zod schema (`.parse` / `.safeParse`) or a type guard forces validation
//! at the boundary — the place where untrusted data enters the program.
//!
//! Detection: walk `call_expression` whose callee is the member expression
//! `JSON.parse`. Allow the call when its grandparent is itself a call
//! whose method name is `parse` or `safeParse` (Zod / schema wrapping).
//! Otherwise flag it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

fn is_json_parse_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    obj.utf8_text(source).unwrap_or("") == "JSON"
        && prop.utf8_text(source).unwrap_or("") == "parse"
}

/// Return true when `call` is being passed as an argument to an enclosing
/// `.parse(...)` / `.safeParse(...)` call — already validated.
fn is_wrapped_in_validator(call: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk up: arguments -> call_expression (the wrapping call).
    let Some(args) = call.parent() else {
        return false;
    };
    if args.kind() != "arguments" {
        return false;
    }
    let Some(outer_call) = args.parent() else {
        return false;
    };
    if outer_call.kind() != "call_expression" {
        return false;
    }
    let Some(outer_fn) = outer_call.child_by_field_name("function") else {
        return false;
    };
    if outer_fn.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = outer_fn.child_by_field_name("property") else {
        return false;
    };
    let method = prop.utf8_text(source).unwrap_or("");
    method == "parse" || method == "safeParse"
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        if !is_json_parse_call(node, source) {
            return;
        }
        if is_wrapped_in_validator(node, source) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`JSON.parse()` returns `any` — wrap it with a Zod schema or type guard before using the result.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_bare_variable_declaration() {
        assert_eq!(run("const data = JSON.parse(body);").len(), 1);
    }

    #[test]
    fn flags_return_statement() {
        assert_eq!(
            run("function f(s: string) { return JSON.parse(s); }").len(),
            1
        );
    }

    #[test]
    fn allows_zod_parse_wrapper() {
        assert!(run("const data = schema.parse(JSON.parse(body));").is_empty());
    }

    #[test]
    fn allows_zod_safe_parse_wrapper() {
        assert!(run("const data = schema.safeParse(JSON.parse(body));").is_empty());
    }

    #[test]
    fn flags_bare_return_in_handler() {
        assert_eq!(
            run("function handler(str: string) { return JSON.parse(str); }").len(),
            1
        );
    }
}
