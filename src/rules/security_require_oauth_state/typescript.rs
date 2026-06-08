//! security-require-oauth-state backend —
//! OAuth callback route handlers that never read/validate `state`.

use crate::diagnostic::{Diagnostic, Severity};

fn strip_comments(text: &str) -> String {
    // Remove block comments first, then line comments.
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn validates_state(text: &str) -> bool {
    // Compaction: drop whitespace so `state ===`, `state !==`, etc. match
    // regardless of formatting.
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("state===")
        || compact.contains("state!==")
        || compact.contains("state==")
        || compact.contains("state!=")
        || compact.contains("===state")
        || compact.contains("!==state")
        || compact.contains("==state")
        || compact.contains("!=state")
    {
        return true;
    }
    // Property reads: `req.query.state`, `params.state`, `searchParams.get("state")`.
    if compact.contains(".state")
        || compact.contains("[\"state\"]")
        || compact.contains("['state']")
    {
        return true;
    }
    if compact.contains(".get(\"state\")") || compact.contains(".get('state')") {
        return true;
    }
    // Passed to a verify/validate/check helper: `verifyState(...)`, `validateState(...)`,
    // or `verify(state)`, `validate(state)`, `check(state)`.
    let lower = compact.to_ascii_lowercase();
    if lower.contains("verifystate")
        || lower.contains("validatestate")
        || lower.contains("checkstate")
        || lower.contains("assertstate")
    {
        return true;
    }
    for marker in ["verify(", "validate(", "check(", "assert("] {
        if let Some(idx) = lower.find(marker) {
            let tail = &lower[idx + marker.len()..];
            if tail.starts_with("state") {
                return true;
            }
        }
    }
    false
}

fn is_oauth_callback_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/callback")
        || lower.contains("/oauth/callback")
        || lower.contains("/auth/callback")
        || lower.ends_with("/cb")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    let is_route_reg = name.ends_with(".get")
        || name.ends_with(".post")
        || name.ends_with(".all");
    if !is_route_reg {
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
    let Some(path_node) = positional.first() else {
        return;
    };
    if path_node.kind() != "string" {
        return;
    }
    let Ok(path_text) = path_node.utf8_text(source) else {
        return;
    };
    if !is_oauth_callback_path(path_text) {
        return;
    }

    // Handler body must compare/validate `state`, not just mention it in
    // a comment or unrelated identifier. Strip line/block comments before
    // pattern-matching so a `// state ignored` comment never counts.
    let mut reads_state = false;
    for arg in positional.iter().skip(1) {
        let Ok(text) = arg.utf8_text(source) else {
            continue;
        };
        let stripped = strip_comments(text);
        if validates_state(&stripped) {
            reads_state = true;
            break;
        }
    }
    if reads_state {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "OAuth callback handler {path_text} never reads `state` — CSRF validation is missing."
        ),
        Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_callback_without_state() {
        let src = "app.get('/auth/callback', (req, res) => { exchange(req.query.code); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_callback_validating_state() {
        let src =
            "app.get('/auth/callback', (req, res) => { if (req.query.state !== saved) throw 0; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_callback_paths() {
        assert!(run("app.get('/widgets', listWidgets);").is_empty());
    }

    #[test]
    fn flags_callback_with_only_state_in_comment() {
        // Bare textual occurrence of `state` (here in a comment) must not
        // count as CSRF validation — only an actual comparison/use does.
        let src = "app.get('/auth/callback', (req, res) => { /* TODO: validate state */ exchange(req.query.code); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_callback_passing_state_to_verify() {
        let src = "app.get('/auth/callback', (req, res) => { verifyState(req.query.state); });";
        assert!(run(src).is_empty());
    }
}
