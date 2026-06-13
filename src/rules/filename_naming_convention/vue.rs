//! filename-naming-convention — Vue backend (PascalCase or kebab-case).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_pascal_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    let mut has_lower = false;
    for &b in bytes.iter().skip(1) {
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            has_lower = true;
        } else if b.is_ascii_uppercase() {
            // OK
        } else {
            return false;
        }
    }
    has_lower
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if super::is_sveltekit_route_file(file_name) {
            return Vec::new();
        }
        if stem.is_empty() || is_pascal_case(stem) || super::text::is_kebab_case(stem) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!(
                "Vue SFC `{file_name}` should use PascalCase (e.g. `UserProfile.vue`) or kebab-case (e.g. `user-profile.vue`)."
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
    fn allows_pascal_case() {
        assert!(run("src/components/UserProfile.vue").is_empty());
    }

    #[test]
    fn allows_single_word_pascal() {
        assert!(run("src/App.vue").is_empty());
    }

    #[test]
    fn allows_multi_word_pascal() {
        assert!(run("src/components/Hook0CardHeader.vue").is_empty());
    }

    #[test]
    fn allows_sveltekit_page_component() {
        assert!(run("src/routes/users/+page.svelte").is_empty());
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("src/components/user-profile.vue").is_empty());
    }

    // Regression for #1424: kebab-case Vue SFC filenames are endorsed by the
    // official Vue style guide and must not be flagged.
    #[test]
    fn allows_kebab_case_panel_issue_1424() {
        assert!(run("app/src/panels/time-series/panel-time-series.vue").is_empty());
    }

    #[test]
    fn allows_kebab_case_app_root_issue_1424() {
        assert!(run("app/src/app.vue").is_empty());
    }

    // Regression for #1820: `index.vue` is the Vue Router / Nuxt file-system
    // routing convention for a directory's entry component — the same
    // index-as-entry convention as `index.ts`. Its `index` stem is lowercase,
    // so the kebab-case allowance must keep it from being flagged.
    #[test]
    fn allows_index_route_file_issue_1820() {
        assert!(run("demos/preview/index.vue").is_empty());
        assert!(run("demos/src/Marks/Underline/Vue/index.vue").is_empty());
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/components/user_profile.vue").len(), 1);
    }

    #[test]
    fn flags_camel_case() {
        assert_eq!(run("src/components/userProfile.vue").len(), 1);
    }
}
