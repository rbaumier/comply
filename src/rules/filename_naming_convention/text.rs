use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` when `stem` matches kebab-case: a lowercase ASCII letter
/// followed by lowercase alphanumerics optionally separated by single dashes.
/// Equivalent to the pattern `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
fn is_kebab_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_dash = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'-' {
            if prev_dash || i == 0 || i == bytes.len() - 1 {
                return false;
            }
            prev_dash = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_dash = false;
        } else {
            return false;
        }
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        // Use the portion before the first `.` as the stem so that multi-part
        // extensions like `.d.ts`, `.test.ts`, `.spec.tsx` don't contaminate
        // the name check.
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return Vec::new();
        }
        if is_kebab_case(stem) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!(
                "Filename `{file_name}` does not match kebab-case convention."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), ""))
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("src/user-profile.ts").is_empty());
    }

    #[test]
    fn allows_single_word_lowercase() {
        assert!(run("src/index.ts").is_empty());
    }

    #[test]
    fn allows_kebab_case_with_digits() {
        assert!(run("src/oauth2-provider.ts").is_empty());
    }

    #[test]
    fn flags_camel_case() {
        assert_eq!(run("src/userProfile.ts").len(), 1);
    }

    #[test]
    fn flags_pascal_case() {
        assert_eq!(run("src/UserProfile.ts").len(), 1);
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    #[test]
    fn flags_trailing_dash() {
        assert_eq!(run("src/user-.ts").len(), 1);
    }

    #[test]
    fn flags_double_dash() {
        assert_eq!(run("src/user--profile.ts").len(), 1);
    }
}
