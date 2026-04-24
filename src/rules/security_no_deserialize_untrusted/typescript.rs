//! security-no-deserialize-untrusted backend — flag unsafe deserialization
//! calls whose argument looks like user-controlled input.

use crate::diagnostic::{Diagnostic, Severity};

fn is_unsafe_deserializer(name: &str) -> bool {
    matches!(
        name,
        "unserialize"
            | "deserialize"
            | "nodeSerialize.unserialize"
            | "serialize.unserialize"
            | "yaml.load"
            | "YAML.load"
            | "pickle.loads"
            | "pickle.load"
    )
}

fn looks_like_user_input(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("req.body")
        || lower.contains("req.query")
        || lower.contains("req.params")
        || lower.contains("req.headers")
        || lower.contains("req.cookies")
        || lower.contains("request.body")
        || lower.contains("request.query")
        || lower.contains("ctx.request")
        || lower.contains("event.body")
        || lower.contains("userinput")
        || lower.contains("user_input")
        || lower.contains("untrusted")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !is_unsafe_deserializer(name) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    let Some(first_arg) = positional.first() else {
        return;
    };
    let Ok(arg_text) = first_arg.utf8_text(source) else {
        return;
    };
    if !looks_like_user_input(arg_text) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{name}` on user-controlled input enables remote code execution — use a safe parser."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unserialize_on_req_body() {
        assert_eq!(run("unserialize(req.body.payload);").len(), 1);
    }

    #[test]
    fn flags_yaml_load_on_req_query() {
        assert_eq!(run("yaml.load(req.query.config);").len(), 1);
    }

    #[test]
    fn allows_unserialize_on_constant() {
        assert!(run("unserialize('fixed-string');").is_empty());
    }

    #[test]
    fn ignores_safe_parsers() {
        assert!(run("JSON.parse(req.body.payload);").is_empty());
    }
}
