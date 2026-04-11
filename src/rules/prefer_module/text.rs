//! prefer-module backend — flag CommonJS patterns in ESM-capable files.
//!
//! Detected patterns:
//! - `require("…")` calls
//! - `module.exports`
//! - `exports.foo`
//! - `__dirname`
//! - `__filename`

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// CommonJS identifiers and their ESM replacements.
const CJS_GLOBALS: &[(&str, &str)] = &[
    ("__dirname", "Use `import.meta.dirname` instead of `__dirname`."),
    ("__filename", "Use `import.meta.filename` instead of `__filename`."),
];

/// True if the character is valid in a JS identifier.
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Check if `needle` appears as a standalone identifier in `line` (not part
/// of a larger word).
fn has_standalone(line: &str, needle: &str) -> bool {
    let bytes = line.as_bytes();
    let nlen = needle.len();
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_ident_char(bytes[abs - 1]);
        let after_ok = abs + nlen >= bytes.len() || !is_ident_char(bytes[abs + nlen]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + nlen;
    }
    false
}

/// True if the line contains a `require(` call as a standalone identifier.
fn has_require_call(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find("require(") {
        let abs = start + pos;
        if abs == 0 || !is_ident_char(bytes[abs - 1]) {
            return true;
        }
        start = abs + 8;
    }
    false
}

/// True if the line contains `module.exports`.
fn has_module_exports(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("module.exports") {
        let abs = start + pos;
        let bytes = line.as_bytes();
        let before_ok = abs == 0 || !is_ident_char(bytes[abs - 1]);
        let after_pos = abs + "module.exports".len();
        let after_ok = after_pos >= bytes.len() || !is_ident_char(bytes[after_pos]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + "module.exports".len();
    }
    false
}

/// True if the line contains `exports.` as a standalone member expression
/// (not `module.exports`).
fn has_exports_member(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find("exports.") {
        let abs = start + pos;
        // Skip `module.exports.`
        if abs >= 7 && &line[abs - 7..abs] == "module." {
            start = abs + 8;
            continue;
        }
        let before_ok = abs == 0 || !is_ident_char(bytes[abs - 1]);
        if before_ok {
            return true;
        }
        start = abs + 8;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Skip .cjs files — CommonJS is expected there.
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

            // Check require()
            if has_require_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-module".into(),
                    message: "Use `import` instead of `require()` — prefer ESM over CommonJS."
                        .into(),
                    severity: Severity::Warning,
                });
                continue;
            }

            // Check module.exports
            if has_module_exports(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-module".into(),
                    message: "Use `export` instead of `module.exports` — prefer ESM over CommonJS."
                        .into(),
                    severity: Severity::Warning,
                });
                continue;
            }

            // Check exports.foo
            if has_exports_member(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-module".into(),
                    message:
                        "Use `export` instead of `exports.x = …` — prefer ESM over CommonJS."
                            .into(),
                    severity: Severity::Warning,
                });
                continue;
            }

            // Check __dirname / __filename
            for &(global, msg) in CJS_GLOBALS {
                if has_standalone(trimmed, global) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "prefer-module".into(),
                        message: msg.into(),
                        severity: Severity::Warning,
                    });
                    break;
                }
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
    fn flags_require() {
        let d = run(r#"const fs = require("fs");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("require()"));
    }

    #[test]
    fn flags_module_exports() {
        let d = run("module.exports = foo;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }

    #[test]
    fn flags_exports_member() {
        let d = run("exports.bar = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("exports.x"));
    }

    #[test]
    fn flags_dirname() {
        let d = run("const dir = __dirname;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn flags_filename() {
        let d = run("const file = __filename;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.filename"));
    }

    #[test]
    fn skips_cjs_files() {
        assert!(run_cjs(r#"const fs = require("fs");"#).is_empty());
    }

    #[test]
    fn skips_comments() {
        assert!(run("// require('fs')").is_empty());
    }

    #[test]
    fn allows_esm_import() {
        assert!(run(r#"import fs from "node:fs";"#).is_empty());
    }

    #[test]
    fn does_not_flag_exports_inside_module_exports() {
        let d = run("module.exports = foo;");
        // Should flag module.exports, not also exports.
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }
}
