use crate::diagnostic::{Diagnostic, Severity};

const BANNED_VERBS: &[&str] = &[
    "reads",
    "pulls",
    "fetches",
    "loads",
    "sums",
    "counts",
    "aggregates",
    "iterates",
];

fn first_word(body: &str) -> Option<String> {
    body.split_whitespace().next().map(|w| {
        w.trim_matches(|c: char| !c.is_ascii_alphabetic())
            .to_lowercase()
    })
}

fn strip_markers(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.lines() {
        let t = line
            .trim()
            .trim_start_matches("/**")
            .trim_start_matches("///")
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();
        if !t.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(t);
        }
    }
    out
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration" | "method_definition" | "variable_declarator"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_function_like(node.kind()) { return; }
    if node.kind() == "variable_declarator" {
        let value = node.child_by_field_name("value").map(|v| v.kind());
        if !matches!(value, Some("arrow_function") | Some("function_expression")) { return; }
    }
    let anchor = if node.kind() == "variable_declarator" {
        node.parent().unwrap_or(node)
    } else {
        node
    };
    let Some(prev) = anchor.prev_named_sibling() else { return };
    if prev.kind() != "comment" { return; }
    let Ok(raw) = prev.utf8_text(source) else { return };
    let body = strip_markers(raw);
    let Some(first) = first_word(&body) else { return };
    if !BANNED_VERBS.contains(&first.as_str()) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &prev,
        super::META.id,
        format!("Docstring opens with `{first}` — start with intent, not implementation (e.g. `Return…`, `Ensure…`)."),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_reads_verb() {
        let src = "/** Reads the user from storage */\nfunction loadUser() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_iterates_verb() {
        let src = "// iterates over nodes\nfunction walk() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_intent_verb() {
        let src =
            "/** Return the current user, creating one if missing. */\nfunction loadUser() {}";
        assert!(run(src).is_empty());
    }
}
