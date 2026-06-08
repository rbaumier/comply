//! OxcCheck backend for prefer-node-protocol.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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

fn is_bare_builtin(specifier: &str) -> bool {
    if specifier.starts_with("node:") {
        return false;
    }
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root)
}

fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::ExportNamedDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Skip .cjs files.
        if ctx
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cjs"))
        {
            return;
        }

        let specifier = match node.kind() {
            AstKind::ImportDeclaration(import) => {
                strip_quotes(&import.source.value)
            }
            AstKind::ExportNamedDeclaration(export) => {
                let Some(ref source) = export.source else { return };
                strip_quotes(&source.value)
            }
            AstKind::CallExpression(call) => {
                // require('...')
                if !call.callee.is_specific_id("require") {
                    return;
                }
                let Some(arg) = call.arguments.first() else { return };
                let Some(expr) = arg.as_expression() else { return };
                let Expression::StringLiteral(lit) = expr else { return };
                &lit.value
            }
            _ => return,
        };

        if !is_bare_builtin(specifier) {
            return;
        }

        let span = match node.kind() {
            AstKind::ImportDeclaration(n) => n.span,
            AstKind::ExportNamedDeclaration(n) => n.span,
            AstKind::CallExpression(n) => n.span,
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `node:{specifier}` over `{specifier}` — the `node:` prefix \
                 makes it unambiguous that this is a Node.js builtin."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstCheck;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
    fn flags_export_from() {
        let d = run(r#"export { createServer } from "http";"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn skips_comments() {
        // `// import fs from "fs";` is a comment node — its children are
        // not parsed as `import_statement`, so the rule never fires.
        assert!(run(r#"// import fs from "fs";"#).is_empty());
    }
}
