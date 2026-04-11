use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

fn is_node_builtin(specifier: &str) -> bool {
    if let Some(rest) = specifier.strip_prefix("node:") {
        // node:fs, node:path, etc. — all valid
        return !rest.is_empty();
    }
    // Check root module name (e.g. "fs/promises" -> "fs")
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root)
}

/// Extract the module specifier from an import line.
/// Matches: `import ... from 'spec'` / `import ... from "spec"` / `import 'spec'` / `import "spec"`
fn extract_import_specifier(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") && !trimmed.starts_with("import\t") {
        return None;
    }
    // Find the last quoted string on the line — that's the specifier
    let spec = extract_quoted(trimmed)?;
    Some(spec)
}

fn extract_quoted(s: &str) -> Option<&str> {
    // Try single quotes first, then double quotes — pick the last occurrence
    let single = s.rfind('\'').and_then(|end| {
        let before = &s[..end];
        let start = before.rfind('\'')?;
        Some(&s[start + 1..end])
    });
    let double = s.rfind('"').and_then(|end| {
        let before = &s[..end];
        let start = before.rfind('"')?;
        Some(&s[start + 1..end])
    });
    // Return whichever appears later in the string (the from-specifier, not a type string)
    match (single, double) {
        (Some(a), Some(b)) => {
            let a_pos = s.rfind(&format!("'{a}'")).unwrap_or(0);
            let b_pos = s.rfind(&format!("\"{b}\"")).unwrap_or(0);
            if a_pos > b_pos {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.starts_with('.') && !spec.starts_with('/')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(spec) = extract_import_specifier(line)
                && is_bare_specifier(spec) && !is_node_builtin(spec) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-implicit-deps".into(),
                        message: format!(
                            "Bare import `{spec}` is not a Node.js builtin — ensure it is listed in package.json."
                        ),
                        severity: Severity::Warning,
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

    #[test]
    fn flags_bare_specifier() {
        assert_eq!(run("import { foo } from 'lodash';").len(), 1);
    }

    #[test]
    fn flags_scoped_package() {
        assert_eq!(run("import { bar } from '@acme/utils';").len(), 1);
    }

    #[test]
    fn allows_relative_import() {
        assert!(run("import { foo } from './utils';").is_empty());
    }

    #[test]
    fn allows_node_builtin() {
        assert!(run("import fs from 'fs';").is_empty());
    }

    #[test]
    fn allows_node_prefixed() {
        assert!(run("import { readFile } from 'node:fs';").is_empty());
    }

    #[test]
    fn allows_node_builtin_subpath() {
        assert!(run("import { readFile } from 'fs/promises';").is_empty());
    }
}
