//! import-enforce-node-protocol-usage oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Bare Node built-in module names that should be prefixed `node:`.
/// Limited to the canonical built-ins — third-party packages with the
/// same name (e.g. someone publishes `crypto` on npm) must NOT use the
/// protocol prefix, so we keep the list conservative.
const NODE_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "crypto",
    "dgram",
    "dns",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "timers",
    "timers/promises",
    "tls",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let specifier = import.source.value.as_str();
        if specifier.starts_with("node:") {
            return;
        }
        if !NODE_BUILTINS.contains(&specifier) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Native Node module `{specifier}` should be imported with the `node:` \
                 prefix: `import … from \"node:{specifier}\"`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_unprefixed_fs_import() {
        let src = r#"import fs from "fs";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unprefixed_path_import() {
        let src = r#"import { join } from "path";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_node_prefixed_import() {
        let src = r#"import fs from "node:fs";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_third_party_imports() {
        let src = r#"import express from "express";"#;
        assert!(run(src).is_empty());
    }
}
