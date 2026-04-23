use std::collections::HashSet;
use std::path::Component;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Folder segments whose subtree should be skipped entirely — vendored
/// dependencies, build artefacts, coverage output, etc. Anything living under
/// one of these is not our source code.
fn is_skipped_subtree(segment: &str) -> bool {
    let roots: HashSet<&str> = [
        "node_modules",
        "target",
        "dist",
        "build",
        "out",
        "coverage",
        "vendor",
        // OS temp directories — tests run here, not real source code.
        "var",
        "tmp",
        "private",
        "temp",
        "T", // macOS /var/folders/.../T/
    ]
    .iter()
    .copied()
    .collect();
    roots.contains(segment)
}

/// Folder segments that are allowed to violate kebab-case because they are
/// conventional (VCS/tooling dotfile folders, Jest-style `__tests__` /
/// `__mocks__` fixtures, …). Unlike `is_skipped_subtree`, descendants of
/// these are still checked.
fn is_exception(segment: &str) -> bool {
    // Always skip dotfile/hidden folders (`.git`, `.github`, `.vscode`, …).
    if segment.starts_with('.') {
        return true;
    }
    // Always skip folders wrapped in double underscores (`__tests__`,
    // `__mocks__`, `__snapshots__`, …) — Jest/Babel convention.
    if segment.starts_with("__") && segment.ends_with("__") {
        return true;
    }
    false
}

/// Returns `true` when `segment` matches kebab-case:
/// `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
fn is_kebab_case(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let bytes = segment.as_bytes();
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
        // Iterate the directory components only (skip the file name itself).
        let parent = match ctx.path.parent() {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut seen: HashSet<String> = HashSet::new();
        let mut diagnostics = Vec::new();

        for component in parent.components() {
            let Component::Normal(os) = component else {
                continue;
            };
            let Some(segment) = os.to_str() else { continue };
            if segment.is_empty() {
                continue;
            }
            // Once we enter a skipped subtree (node_modules, target, …) the
            // whole descendant path is vendor/build output — bail out.
            if is_skipped_subtree(segment) {
                return Vec::new();
            }
            if is_exception(segment) {
                continue;
            }
            if is_kebab_case(segment) {
                continue;
            }
            // De-duplicate identical offending segments per file.
            if !seen.insert(segment.to_string()) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: "folder-naming-convention".into(),
                message: format!(
                    "Folder `{segment}` does not match kebab-case convention."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
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
    fn allows_kebab_case_folders() {
        assert!(run("src/user-profile/index.ts").is_empty());
    }

    #[test]
    fn allows_single_word_lowercase_folders() {
        assert!(run("src/rules/mod.rs").is_empty());
    }

    #[test]
    fn allows_kebab_case_with_digits() {
        assert!(run("src/oauth2-provider/token.ts").is_empty());
    }

    #[test]
    fn flags_camel_case_folder() {
        let diags = run("src/userProfile/index.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("userProfile"));
    }

    #[test]
    fn flags_pascal_case_folder() {
        let diags = run("src/UserProfile/index.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("UserProfile"));
    }

    #[test]
    fn flags_snake_case_folder() {
        let diags = run("src/user_profile/index.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("user_profile"));
    }

    #[test]
    fn skips_node_modules() {
        assert!(run("node_modules/someLib/index.js").is_empty());
    }

    #[test]
    fn skips_dot_folders() {
        assert!(run(".git/hooks/file").is_empty());
        assert!(run(".github/workflows/ci.yml").is_empty());
    }

    #[test]
    fn skips_double_underscore_folders() {
        assert!(run("src/__tests__/foo.test.ts").is_empty());
        assert!(run("src/__mocks__/api.ts").is_empty());
    }

    #[test]
    fn flags_multiple_bad_segments() {
        let diags = run("src/BadFolder/anotherBad/file.ts");
        assert_eq!(diags.len(), 2);
    }
}
