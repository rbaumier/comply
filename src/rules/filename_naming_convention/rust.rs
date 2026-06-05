use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_snake_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_underscore = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            if prev_underscore || i == 0 {
                return false;
            }
            prev_underscore = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_underscore = false;
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
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return Vec::new();
        }
        if is_snake_case(stem) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!("Filename `{file_name}` does not match snake_case convention."),
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
    fn allows_snake_case() {
        assert!(run("src/e2e_cli.rs").is_empty());
    }

    #[test]
    fn allows_single_word() {
        assert!(run("src/main.rs").is_empty());
    }

    #[test]
    fn allows_snake_case_with_digits() {
        assert!(run("src/oauth2_provider.rs").is_empty());
    }

    #[test]
    fn flags_kebab_case() {
        assert_eq!(run("src/e2e-cli.rs").len(), 1);
    }

    #[test]
    fn flags_camel_case() {
        assert_eq!(run("src/userProfile.rs").len(), 1);
    }

    #[test]
    fn flags_pascal_case() {
        assert_eq!(run("src/UserProfile.rs").len(), 1);
    }

    #[test]
    fn allows_trailing_underscore() {
        assert!(run("src/user_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_struct() {
        assert!(run("src/de/struct_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_type() {
        assert!(run("src/type_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_match() {
        assert!(run("src/match_.rs").is_empty());
    }

    #[test]
    fn flags_double_underscore() {
        assert_eq!(run("src/user__profile.rs").len(), 1);
    }
}
