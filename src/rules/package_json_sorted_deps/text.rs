//! package-json-sorted-deps backend — flag unsorted dependency keys in
//! package.json files.
//!
//! Text-based: we look for `"dependencies"`, `"devDependencies"`,
//! `"peerDependencies"` blocks and check that the `"package-name"` keys
//! within each block are in alphabetical (case-insensitive) order.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DEP_SECTIONS: &[&str] = &[
    "\"dependencies\"",
    "\"devDependencies\"",
    "\"peerDependencies\"",
];

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> { Some(&["dependencies", "devDependencies", "peerDependencies"]) }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only apply to package.json files.
        if ctx.path.file_name().and_then(|f| f.to_str()) != Some("package.json") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            // Detect a dependency section header.
            let is_dep_section = DEP_SECTIONS.iter().any(|s| line.contains(s));
            if !is_dep_section {
                i += 1;
                continue;
            }

            let section_line = i + 1; // 1-indexed
            // Find the opening brace (might be on the same line or the next).
            let mut brace_line = i;
            if !line.contains('{') {
                brace_line = i + 1;
            }
            if brace_line != i && (brace_line >= lines.len() || !lines[brace_line].contains('{')) {
                i += 1;
                continue;
            }

            // Collect package names until closing brace.
            let mut keys: Vec<String> = Vec::new();
            let mut j = brace_line + 1;
            while j < lines.len() {
                let dep_line = lines[j].trim();
                if dep_line.contains('}') {
                    break;
                }
                // Extract the key: "package-name": "version"
                if let Some(start) = dep_line.find('"')
                    && let Some(end) = dep_line[start + 1..].find('"')
                {
                    keys.push(dep_line[start + 1..start + 1 + end].to_string());
                }
                j += 1;
            }

            // Check alphabetical order.
            for w in keys.windows(2) {
                if w[0].to_lowercase() > w[1].to_lowercase() {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: section_line,
                        column: 1,
                        rule_id: "package-json-sorted-deps".into(),
                        message: format!(
                            "Dependencies are not sorted alphabetically: \
                             `{}` should come before `{}`.",
                            w[1], w[0],
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break; // One diagnostic per section.
                }
            }

            i = j + 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("package.json"), source))
    }

    fn run_other(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("index.ts"), source))
    }

    #[test]
    fn flags_unsorted_deps() {
        let src = r#"{
  "dependencies": {
    "zod": "^3.0.0",
    "axios": "^1.0.0"
  }
}"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("axios"));
    }

    #[test]
    fn allows_sorted_deps() {
        let src = r#"{
  "dependencies": {
    "axios": "^1.0.0",
    "zod": "^3.0.0"
  }
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_package_json() {
        let src = r#"{
  "dependencies": {
    "zod": "^3.0.0",
    "axios": "^1.0.0"
  }
}"#;
        assert!(run_other(src).is_empty());
    }

    #[test]
    fn flags_unsorted_dev_deps() {
        let src = r#"{
  "devDependencies": {
    "vitest": "^1.0.0",
    "eslint": "^8.0.0"
  }
}"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
