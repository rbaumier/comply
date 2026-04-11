use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Test file suffixes to look for.
const TEST_SUFFIXES: &[&str] = &[".test.ts", ".test.tsx", ".spec.ts", ".spec.tsx"];

/// Patterns in the path that indicate the file should be skipped.
const SKIP_PATH_PATTERNS: &[&str] = &[".test.", ".spec.", ".d.ts", "node_modules"];

/// File stems that are skipped (index files, config files).
const SKIP_STEMS: &[&str] = &[
    "index",
    "main",
    "types",
    "constants",
    "config",
    "env",
    "vite.config",
    "tsconfig",
    "jest.config",
    "vitest.config",
    "tailwind.config",
    "next.config",
    "eslint.config",
    "prettier.config",
];

fn should_skip(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    if SKIP_PATH_PATTERNS.iter().any(|p| path_str.contains(p)) {
        return true;
    }
    // Skip based on file stem
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        return SKIP_STEMS.contains(&stem);
    }
    false
}

fn is_ts_source(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "ts" | "tsx")
}

/// Check if the file is type-only (only `type`, `interface`, `export type`,
/// `export interface`, comments, imports, and blank lines).
fn is_type_only(source: &str) -> bool {
    let mut brace_depth: i32 = 0;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("*/")
        {
            continue;
        }
        // Top-level type constructs
        if brace_depth == 0
            && (trimmed.starts_with("import ")
                || trimmed.starts_with("export type ")
                || trimmed.starts_with("export interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("interface "))
        {
            // Track braces opened on this line
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                }
            }
            continue;
        }
        // Inside a type/interface body — allow anything
        if brace_depth > 0 {
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                }
            }
            continue;
        }
        // Non-type content at top level
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_ts_source(ctx.path) || should_skip(ctx.path) {
            return Vec::new();
        }

        // Skip type-only files
        if is_type_only(ctx.source) {
            return Vec::new();
        }

        let stem = ctx.path.file_stem().and_then(|s| s.to_str());
        let parent = ctx.path.parent();

        if let (Some(stem), Some(dir)) = (stem, parent) {
            // Check if any test file exists
            let has_test = TEST_SUFFIXES.iter().any(|suffix| {
                let test_file = dir.join(format!("{stem}{suffix}"));
                test_file.exists()
            });

            if !has_test {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: 1,
                    column: 1,
                    rule_id: "colocated-tests".into(),
                    message: format!(
                        "No colocated test file found for `{}` — expected `{stem}.test.ts` or `{stem}.spec.ts`.",
                        ctx.path.file_name().unwrap_or_default().to_string_lossy(),
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
        Check.check(&CheckCtx::for_test(path, "export function foo() {}"))
    }

    fn run_with_source(path: &Path, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(path, source))
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
    fn skips_index_files() {
        let d = run(Path::new("src/index.ts"));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_config_files() {
        let d = run(Path::new("src/config.ts"));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_type_only_files() {
        let src = "\
export type Foo = { bar: string };
export interface Baz {
  qux: number;
}
";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.ts");
        fs::write(&path, src).unwrap();
        let d = run_with_source(&path, src);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_source_without_test() {
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

    #[test]
    fn allows_source_with_spec() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("utils.ts");
        let spec = dir.path().join("utils.spec.ts");
        fs::write(&src, "export function foo() {}").unwrap();
        fs::write(&spec, "test('foo', () => {})").unwrap();

        let d = run(&src);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_d_ts_files() {
        let d = run(Path::new("src/types.d.ts"));
        assert!(d.is_empty());
    }
}
