use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect files that import `assert` from `node:assert` / `assert` / `node:assert/strict`
/// and then call it as a bare function `assert(…)` instead of `assert.ok(…)`.
///
/// We do a two-pass scan:
/// 1. Check if the file imports the default assert function.
/// 2. Find bare `assert(` calls (not `assert.something(`).
impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Pass 1: does this file import `assert` from node:assert?
        let mut has_assert_import = false;
        let mut assert_name: Option<String> = None;

        for line in ctx.source.lines() {
            let trimmed = line.trim();
            // Match: import assert from 'node:assert'
            //        import assert from 'assert'
            //        import assert from 'node:assert/strict'
            //        import { strict as assert } from 'node:assert'
            //        import { default as assert } from 'node:assert'
            let is_assert_module = trimmed.contains("'node:assert'")
                || trimmed.contains("\"node:assert\"")
                || trimmed.contains("'assert'")
                || trimmed.contains("\"assert\"")
                || trimmed.contains("'node:assert/strict'")
                || trimmed.contains("\"node:assert/strict\"")
                || trimmed.contains("'assert/strict'")
                || trimmed.contains("\"assert/strict\"");

            if !is_assert_module || !trimmed.starts_with("import ") {
                continue;
            }

            // Extract the local name. Handle:
            // - `import assert from '...'`    → name = "assert"
            // - `import foo from '...'`       → name = "foo"
            // - `import { strict as bar } from 'assert'` → name = "bar"
            // - `import { default as baz } from '...'`   → name = "baz"
            let after_import = &trimmed["import ".len()..];

            if after_import.starts_with('{') {
                // Named import: `import { strict as X }` or `import { default as X }`
                if let Some(as_pos) = after_import.find(" as ") {
                    let after_as = &after_import[as_pos + 4..];
                    let name: String = after_as
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                        .collect();
                    if !name.is_empty() {
                        assert_name = Some(name);
                        has_assert_import = true;
                    }
                }
            } else {
                // Default import: `import NAME from '...'`
                let name: String = after_import
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    assert_name = Some(name);
                    has_assert_import = true;
                }
            }
        }

        if !has_assert_import {
            return diagnostics;
        }

        let name = assert_name.unwrap();
        let bare_call = format!("{}(", name);
        let method_call = format!("{}.", name);

        // Pass 2: find bare `assert(` calls that are not `assert.something(`
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Skip import lines
            if trimmed.starts_with("import ") {
                continue;
            }

            let mut search_start = 0;
            while let Some(pos) = line[search_start..].find(&bare_call) {
                let abs = search_start + pos;

                // Check the character before the name to ensure it's a word boundary
                let before_name = if abs > 0 {
                    line.as_bytes().get(abs - 1).copied()
                } else {
                    None
                };
                let is_word_boundary = match before_name {
                    None => true,
                    Some(b) => !b.is_ascii_alphanumeric() && b != b'_' && b != b'$',
                };

                // Check it's not a method call (e.g. `obj.assert(`)
                let is_method = abs > 0 && line[..abs].trim_end().ends_with('.');

                if is_word_boundary && !is_method {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: abs + 1,
                        rule_id: "consistent-assert".into(),
                        message: format!(
                            "Prefer `{}.ok(…)` over bare `{}(…)`.",
                            name, name
                        ),
                        severity: Severity::Warning,
                    });
                }

                search_start = abs + bare_call.len();
            }

            // Suppress false positive: skip lines that are method calls like `assert.strictEqual(`
            let _ = &method_call;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_bare_assert_call() {
        let src = "import assert from 'node:assert';\nassert(x === 42);";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "consistent-assert");
    }

    #[test]
    fn flags_bare_assert_strict() {
        let src = "import assert from 'node:assert/strict';\nassert(value);";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_assert_ok() {
        let src = "import assert from 'node:assert';\nassert.ok(value);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_assert_strict_equal() {
        let src = "import assert from 'node:assert';\nassert.strictEqual(x, 42);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_flag_without_import() {
        let src = "assert(true);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_renamed_import() {
        let src = "import { strict as t } from 'assert';\nt(value);";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }
}
