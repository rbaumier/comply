//! prefer-node-protocol backend — flag bare Node.js builtin imports
//! missing the `node:` protocol prefix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// All Node.js builtin module names that support the `node:` prefix.
const NODE_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// Check if a specifier string (without quotes) is a bare Node.js builtin.
/// Matches both `"fs"` and `"fs/promises"` style sub-paths.
fn is_bare_builtin(specifier: &str) -> bool {
    if specifier.starts_with("node:") {
        return false;
    }
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root)
}

/// Extract the module specifier from a line containing `import`, `export`,
/// or `require`. Returns the content between quotes if found.
fn extract_specifier(line: &str) -> Option<&str> {
    // Match import/export ... from 'specifier' or require('specifier')
    for delimiter in ['"', '\''] {
        // Find the last quoted string on the line (the module specifier
        // in `import X from "spec"` or `require("spec")`).
        if let Some(start) = line.rfind(delimiter) {
            let before = &line[..start];
            if let Some(begin) = before.rfind(delimiter) {
                let specifier = &line[begin + 1..start];
                if !specifier.is_empty() && !specifier.contains(delimiter) {
                    return Some(specifier);
                }
            }
        }
    }
    None
}

/// True if the line is an import/export/require that references a module.
fn is_import_or_require(trimmed: &str) -> bool {
    trimmed.starts_with("import ")
        || trimmed.starts_with("import{")
        || trimmed.starts_with("export ")
        || trimmed.contains("require(")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Skip .cjs files — they legitimately use bare specifiers.
        if ctx
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cjs"))
        {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if !is_import_or_require(trimmed) {
                continue;
            }
            let Some(specifier) = extract_specifier(trimmed) else {
                continue;
            };
            if is_bare_builtin(specifier) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-node-protocol".into(),
                    message: format!(
                        "Prefer `node:{specifier}` over `{specifier}` — the `node:` prefix \
                         makes it unambiguous that this is a Node.js builtin."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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

    fn run_cjs(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.cjs"), source))
    }

    #[test]
    fn flags_bare_fs_import() {
        let d = run(r#"import fs from "fs";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("node:fs"));
    }

    #[test]
    fn flags_bare_path_require() {
        let d = run(r#"const path = require("path");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("node:path"));
    }

    #[test]
    fn allows_node_prefix() {
        assert!(run(r#"import fs from "node:fs";"#).is_empty());
    }

    #[test]
    fn allows_user_package() {
        assert!(run(r#"import lodash from "lodash";"#).is_empty());
    }

    #[test]
    fn flags_sub_path() {
        let d = run(r#"import { readFile } from "fs/promises";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_node_sub_path() {
        assert!(run(r#"import { readFile } from "node:fs/promises";"#).is_empty());
    }

    #[test]
    fn skips_cjs_files() {
        assert!(run_cjs(r#"const fs = require("fs");"#).is_empty());
    }

    #[test]
    fn flags_export_from() {
        let d = run(r#"export { createServer } from "http";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_comments() {
        assert!(run(r#"// import fs from "fs";"#).is_empty());
    }
}
