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

fn is_composable_name(stem: &str) -> bool {
    stem.starts_with("use") && stem.len() > 3 && stem.as_bytes()[3].is_ascii_uppercase()
}

fn is_pascal_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

fn is_camel_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

fn is_ts_or_jsx_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".ts") || s.ends_with(".tsx") || s.ends_with(".js") || s.ends_with(".jsx")
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
        if super::is_sveltekit_route_file(file_name) {
            return Vec::new();
        }
        if super::is_tanstack_pathless_route(ctx.path, file_name) {
            return Vec::new();
        }
        if super::is_tanstack_splat_route(ctx.path, file_name) {
            return Vec::new();
        }
        if is_kebab_case(stem) {
            return Vec::new();
        }
        if is_composable_name(stem) {
            return Vec::new();
        }
        if is_ts_or_jsx_file(ctx.path) && (is_pascal_case(stem) || is_camel_case(stem)) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!("Filename `{file_name}` does not match naming convention."),
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
    fn allows_camel_case_ts() {
        assert!(run("src/userProfile.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_js() {
        assert!(run("src/dynamicMiddleware.js").is_empty());
    }

    #[test]
    fn allows_pascal_case_tsx() {
        assert!(run("src/UserProfile.tsx").is_empty());
    }

    #[test]
    fn allows_pascal_case_ts() {
        assert!(run("src/UserProfile.ts").is_empty());
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    #[test]
    fn allows_sveltekit_route_module() {
        assert!(run("src/routes/users/+page.server.ts").is_empty());
    }

    #[test]
    fn flags_trailing_dash() {
        assert_eq!(run("src/user-.ts").len(), 1);
    }

    #[test]
    fn flags_double_dash() {
        assert_eq!(run("src/user--profile.ts").len(), 1);
    }

    #[test]
    fn allows_composable_camel_case() {
        assert!(run("src/composables/useColorMode.ts").is_empty());
    }

    #[test]
    fn allows_composable_with_queries() {
        assert!(run("src/composables/useServiceTokenQueries.ts").is_empty());
    }

    #[test]
    fn allows_non_composable_camel_case() {
        assert!(run("src/externalLinks.ts").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_tsx() {
        assert!(run("src/app/routes/_authed.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_test_tsx() {
        assert!(run("src/app/routes/_authed.test.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_nested() {
        assert!(run("src/app/routes/dashboard/_layout.tsx").is_empty());
    }

    #[test]
    fn flags_underscore_prefix_outside_routes() {
        assert_eq!(run("src/app/_authed.tsx").len(), 1);
    }

    #[test]
    fn allows_tanstack_splat_route_tsx() {
        assert!(run("src/app/routes/$.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_splat_route_ts() {
        assert!(run("src/app/routes/api/$.ts").is_empty());
    }

    #[test]
    fn flags_dollar_stem_outside_routes() {
        assert_eq!(run("src/app/$.tsx").len(), 1);
    }
}
