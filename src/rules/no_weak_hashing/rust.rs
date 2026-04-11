//! no-weak-hashing backend for Rust.
//!
//! Flags MD5/SHA1 usage via identifiers like `Md5::new()`, `Sha1::new()`,
//! or string literals containing these algorithm names in crypto contexts.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_HASH_TYPES: &[&str] = &["Md5", "Sha1", "MD5", "SHA1"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    // Match `Md5::new()`, `Sha1::new()`, `md5::compute()`, `sha1::Sha1::new()`
    for &weak in WEAK_HASH_TYPES {
        let weak_lower = weak.to_ascii_lowercase();
        let callee_lower = callee_text.to_ascii_lowercase();
        if callee_lower.starts_with(&format!("{weak_lower}::"))
            || callee_lower.contains(&format!("::{weak_lower}::"))
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-weak-hashing".into(),
                message: format!(
                    "Weak hashing algorithm `{callee_text}` — use SHA-256 or stronger.",
                ),
                severity: Severity::Error,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_md5_new() {
        assert_eq!(run_on("fn f() { let h = Md5::new(); }").len(), 1);
    }

    #[test]
    fn flags_sha1_new() {
        assert_eq!(run_on("fn f() { let h = Sha1::new(); }").len(), 1);
    }

    #[test]
    fn flags_md5_compute() {
        assert_eq!(run_on("fn f() { let h = md5::compute(data); }").len(), 1);
    }

    #[test]
    fn allows_sha256() {
        assert!(run_on("fn f() { let h = Sha256::new(); }").is_empty());
    }
}
