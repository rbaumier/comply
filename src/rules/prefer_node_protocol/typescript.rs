//! prefer-node-protocol backend — flag `import` / `export ... from` /
//! `require(...)` whose specifier names a Node.js builtin without the
//! `node:` prefix.
//!
//! Walks every `import_statement`, `export_statement`, and `call_expression`
//! whose callee is the bare identifier `require`. Each of those carries a
//! single string specifier; we strip the quotes, check if its first
//! `/`-segment is a known Node builtin, and emit a diagnostic when it is
//! and there is no `node:` prefix.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
    // Skip .cjs files — they legitimately use bare specifiers.
    if ctx
        .path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cjs"))
    {
        return;
    }

    let specifier_node = match node.kind() {
        "import_statement" | "export_statement" => {
            // `export_statement` only carries a `source` field for
            // re-exports (`export { x } from '...'`); plain `export`
            // declarations have no source and are skipped.
            node.child_by_field_name("source")
        }
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return; };
            let Ok(name) = func.utf8_text(source) else { return; };
            if name != "require" { return; }
            let Some(args) = node.child_by_field_name("arguments") else { return; };
            args.named_child(0).filter(|a| a.kind() == "string")
        }
        _ => return,
    };

    let Some(spec_node) = specifier_node else { return; };
    let Ok(raw) = spec_node.utf8_text(source) else { return; };
    let specifier = strip_quotes(raw);
    if !is_bare_builtin(specifier) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-node-protocol".into(),
        message: format!(
            "Prefer `node:{specifier}` over `{specifier}` — the `node:` prefix \
             makes it unambiguous that this is a Node.js builtin."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstCheck;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_cjs(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        Check.check(&CheckCtx::for_test(Path::new("t.cjs"), source), &tree)
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
        // `// import fs from "fs";` is a comment node — its children are
        // not parsed as `import_statement`, so the rule never fires.
        assert!(run(r#"// import fs from "fs";"#).is_empty());
    }
}
