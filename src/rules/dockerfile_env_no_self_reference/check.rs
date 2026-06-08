use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["env_instruction"] => |node, source, ctx, diagnostics|
    let mut prior_keys: Vec<String> = Vec::new();
    let mut cursor = node.walk();
    for pair in node.children(&mut cursor) {
        if pair.kind() != "env_pair" { continue; }
        let pair_text = pair.utf8_text(source).unwrap_or("");
        // env_pair text is like `KEY=value` or `KEY value` (legacy form).
        let (key, value) = if let Some((k, v)) = pair_text.split_once('=') {
            (k.trim(), v)
        } else if let Some((k, v)) = pair_text.split_once(char::is_whitespace) {
            (k.trim(), v)
        } else {
            continue;
        };
        for prev in &prior_keys {
            let dollar = format!("${prev}");
            let braced = format!("${{{prev}}}");
            if value_references(value, &dollar, prev) || value.contains(&braced) {
                let pos = pair.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "ENV `{key}` references `${prev}` defined in the same instruction; the reference resolves to the OLD value. Split into separate ENVs."
                    ),
                    severity: Severity::Warning,
                    span: Some((pair.byte_range().start, pair.byte_range().len())),
                });
                break;
            }
        }
        prior_keys.push(key.to_string());
    }
}

/// Match `$KEY` only when followed by a non-identifier char (or end), so
/// `$PATH_FOO` doesn't match `$PATH`.
fn value_references(value: &str, dollar_token: &str, key: &str) -> bool {
    let mut start = 0;
    while let Some(idx) = value[start..].find(dollar_token) {
        let abs = start + idx;
        let after = abs + dollar_token.len();
        let next = value.as_bytes().get(after).copied();
        let is_ident_continuation =
            matches!(next, Some(b) if b.is_ascii_alphanumeric() || b == b'_');
        if !is_ident_continuation {
            return true;
        }
        start = abs + dollar_token.len();
        let _ = key;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_self_reference_in_same_env() {
        assert_eq!(run("ENV A=1 B=$A\n").len(), 1);
    }

    #[test]
    fn allows_separate_env_instructions() {
        assert!(run("ENV A=1\nENV B=$A\n").is_empty());
    }
}
