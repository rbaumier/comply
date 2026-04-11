use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Test file suffixes to look for.
const TEST_SUFFIXES: &[&str] = &[".test.ts", ".test.tsx", ".spec.ts", ".spec.tsx"];

/// Patterns that indicate the file is itself a test or should be skipped.
const SKIP_PATTERNS: &[&str] = &[
    ".test.", ".spec.", ".d.ts",
    "node_modules",
];

fn should_skip(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    SKIP_PATTERNS.iter().any(|p| path_str.contains(p))
}

fn is_ts_source(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "ts" | "tsx")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_ts_source(ctx.path) || should_skip(ctx.path) {
            return Vec::new();
        }

        let stem = ctx.path.file_stem().and_then(|s| s.to_str());
        let parent = ctx.path.parent();

        if let (Some(stem), Some(dir)) = (stem, parent) {
            // Check if any test file exists
            let has_test = TEST_SUFFIXES.iter().any(|suffix| {
                let test_file = dir.join(format!("{}{}", stem, suffix));
                test_file.exists()
            });

            if !has_test {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: 1,
                    column: 1,
                    rule_id: "colocated-tests".into(),
                    message: format!(
                        "No colocated test file found for `{}` — expected `{}.test.ts` or `{}.spec.ts`.",
                        ctx.path.file_name().unwrap_or_default().to_string_lossy(),
                        stem,
                        stem,
                    ),
                    severity: Severity::Warning,
                }];
            }
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn run(path: &Path) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(path, "const x = 1;"))
    }

    #[test]
    fn skips_test_files() {
        let d = run(Path::new("src/foo.test.ts"));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_spec_files() {
        let d = run(Path::new("src/foo.spec.ts"));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_node_modules() {
        let d = run(Path::new("node_modules/pkg/index.ts"));
        assert!(d.is_empty());
    }

    #[test]
    fn flags_source_without_test() {
        // Use a temp dir so we can control what files exist
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("utils.ts");
        fs::write(&src, "export function foo() {}").unwrap();

        let d = run(&src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utils.test.ts"));
    }

    #[test]
    fn allows_source_with_test() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("utils.ts");
        let test = dir.path().join("utils.test.ts");
        fs::write(&src, "export function foo() {}").unwrap();
        fs::write(&test, "test('foo', () => {})").unwrap();

        let d = run(&src);
        assert!(d.is_empty());
    }
}
