//! no-error-details-in-response backend — flag response-send calls whose
//! payload references `err.message` / `err.stack` (or aliases), which
//! leaks internal details to clients.

use crate::diagnostic::{Diagnostic, Severity};

const RESPONSE_METHODS: &[&str] = &["json", "send"];

const RESPONSE_CALLS: &[&str] = &["Response.json", "NextResponse.json"];

const ERROR_FIELD_SUFFIXES: &[&str] = &[".message", ".stack"];

const ERROR_VAR_PREFIXES: &[&str] = &["err", "error", "e"];

fn is_response_send(name: &str) -> bool {
    if RESPONSE_CALLS.contains(&name) {
        return true;
    }
    let tail = name.rsplit('.').next().unwrap_or(name);
    RESPONSE_METHODS.contains(&tail)
}

fn text_leaks_error_details(text: &str) -> bool {
    for suffix in ERROR_FIELD_SUFFIXES {
        let mut haystack = text;
        while let Some(idx) = haystack.find(suffix) {
            let prefix = &haystack[..idx];
            let ident_end = prefix
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let ident = &prefix[ident_end..];
            if ERROR_VAR_PREFIXES.iter().any(|p| ident.eq_ignore_ascii_case(p)) {
                return true;
            }
            haystack = &haystack[idx + suffix.len()..];
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_response_send(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Ok(args_text) = args.utf8_text(source) else { return };
    if text_leaks_error_details(args_text) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-error-details-in-response",
            "Sending `err.message`/`err.stack` to the client leaks internal details — use a generic message.".into(),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_err_message_in_res_json() {
        assert_eq!(run_on("res.json({ error: err.message })").len(), 1);
    }

    #[test]
    fn flags_err_stack_in_response_json() {
        assert_eq!(run_on("Response.json({ stack: error.stack })").len(), 1);
    }

    #[test]
    fn allows_generic_error_message() {
        assert!(run_on("res.json({ error: 'Internal Server Error' })").is_empty());
    }

    #[test]
    fn allows_err_message_in_log() {
        assert!(run_on("console.error(err.message)").is_empty());
    }
}
