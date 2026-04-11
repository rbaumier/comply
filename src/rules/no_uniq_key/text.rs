use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Non-stable key generators that produce a new value every render.
const BAD_KEY_PATTERNS: &[&str] = &[
    "Math.random()",
    "Date.now()",
    "uuid()",
    "uuidv4()",
    "nanoid()",
    "crypto.randomUUID()",
];

fn has_bad_key(line: &str) -> bool {
    // Must have `key={`
    let Some(key_pos) = line.find("key={") else {
        return false;
    };
    let after_key = &line[key_pos + 5..];
    // Find closing `}`
    let end = after_key.find('}').unwrap_or(after_key.len());
    let key_value = &after_key[..end];

    for pattern in BAD_KEY_PATTERNS {
        if key_value.contains(pattern) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_bad_key(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-uniq-key".into(),
                    message:
                        "Non-unique key — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render, breaking reconciliation."
                            .into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_math_random_key() {
        assert_eq!(run(r#"  <Item key={Math.random()} />"#).len(), 1);
    }

    #[test]
    fn flags_date_now_key() {
        assert_eq!(run(r#"  <Item key={Date.now()} />"#).len(), 1);
    }

    #[test]
    fn flags_uuid_key() {
        assert_eq!(run(r#"  <Item key={uuid()} />"#).len(), 1);
    }

    #[test]
    fn allows_stable_key() {
        assert!(run(r#"  <Item key={item.id} />"#).is_empty());
    }

    #[test]
    fn allows_index_key() {
        assert!(run(r#"  <Item key={index} />"#).is_empty());
    }
}
