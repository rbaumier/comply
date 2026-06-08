//! package-json-unique-deps backend — flag packages that appear in both
//! `dependencies` and `devDependencies`.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SECTIONS: &[&str] = &["\"dependencies\"", "\"devDependencies\""];

/// Collect package names from a JSON dependency block. Returns the keys
/// found and the line index after the closing brace.
fn collect_keys(lines: &[&str], start: usize) -> (Vec<String>, usize) {
    let mut keys = Vec::new();
    // Find the opening brace.
    let mut brace_line = start;
    if !lines[start].contains('{') {
        brace_line = start + 1;
    }
    if brace_line != start && (brace_line >= lines.len() || !lines[brace_line].contains('{')) {
        return (keys, start + 1);
    }
    let mut j = brace_line + 1;
    while j < lines.len() {
        let dep_line = lines[j].trim();
        if dep_line.contains('}') {
            break;
        }
        if let Some(start_q) = dep_line.find('"')
            && let Some(end_q) = dep_line[start_q + 1..].find('"')
        {
            keys.push(dep_line[start_q + 1..start_q + 1 + end_q].to_string());
        }
        j += 1;
    }
    (keys, j + 1)
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dependencies", "devDependencies"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.file_name().and_then(|f| f.to_str()) != Some("package.json") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut section_keys: FxHashMap<&str, FxHashSet<String>> = FxHashMap::default();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            let mut matched_section = None;
            for &section in SECTIONS {
                if line.contains(section) {
                    matched_section = Some(section);
                    break;
                }
            }
            if let Some(section) = matched_section {
                let (keys, next) = collect_keys(&lines, i);
                section_keys.entry(section).or_default().extend(keys);
                i = next;
            } else {
                i += 1;
            }
        }

        let deps = section_keys
            .get("\"dependencies\"")
            .cloned()
            .unwrap_or_default();
        let dev_deps = section_keys
            .get("\"devDependencies\"")
            .cloned()
            .unwrap_or_default();

        let overlap: Vec<&String> = deps.intersection(&dev_deps).collect();

        overlap
            .iter()
            .map(|pkg| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: 1,
                column: 1,
                rule_id: "package-json-unique-deps".into(),
                message: format!(
                    "`{pkg}` appears in both `dependencies` and `devDependencies` — pick one."
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("package.json"), source))
    }

    #[test]
    fn flags_duplicate_across_sections() {
        let src = r#"{
  "dependencies": {
    "zod": "^3.0.0"
  },
  "devDependencies": {
    "zod": "^3.0.0",
    "vitest": "^1.0.0"
  }
}"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("zod"));
    }

    #[test]
    fn allows_unique_across_sections() {
        let src = r#"{
  "dependencies": {
    "zod": "^3.0.0"
  },
  "devDependencies": {
    "vitest": "^1.0.0"
  }
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_package_json() {
        let src = r#"{
  "dependencies": { "zod": "1" },
  "devDependencies": { "zod": "1" }
}"#;
        let diags = Check.check(&CheckCtx::for_test(Path::new("index.ts"), src));
        assert!(diags.is_empty());
    }
}
