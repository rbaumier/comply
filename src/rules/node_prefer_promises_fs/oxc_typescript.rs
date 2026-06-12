use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use rustc_hash::FxHashSet;
use std::sync::Arc;

const FS_METHODS: &[&str] = &[
    "readFile",
    "writeFile",
    "appendFile",
    "copyFile",
    "mkdir",
    "mkdtemp",
    "open",
    "readdir",
    "readlink",
    "rename",
    "rmdir",
    "rm",
    "stat",
    "lstat",
    "unlink",
    "access",
    "chmod",
    "lchmod",
    "lchown",
    "chown",
    "link",
    "symlink",
    "truncate",
    "realpath",
    "utimes",
];

/// Module specifiers that already expose the promise-based API. A binding
/// imported from one of these is `fs.promises` under another name, so
/// `binding.method()` is not callback-based and must not be flagged.
const PROMISES_FS_MODULES: &[&str] = &["fs/promises", "node:fs/promises"];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fs"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let promises_bindings = collect_promises_fs_bindings(semantic.nodes().program());
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method = member.property.name.as_str();

            // Skip Sync variants — handled by node-no-sync.
            if method.ends_with("Sync") {
                continue;
            }
            if !FS_METHODS.contains(&method) {
                continue;
            }

            // Object must be the bare `fs` identifier (not `fs.promises`).
            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            let obj_name = obj.name.as_str();
            if obj_name != "fs" {
                continue;
            }
            // A binding imported from `fs/promises` already exposes the
            // promise API — `fs.writeFile()` IS `fs.promises.writeFile()`.
            if promises_bindings.contains(obj_name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Use `fs.promises.{method}()` instead of callback-based `fs.{method}()`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// Local names bound to a default or namespace import from a promise-based
/// `fs` module (`import fs from "fs/promises"`, `import * as fs from
/// "node:fs/promises"`).
fn collect_promises_fs_bindings<'a>(program: &Program<'a>) -> FxHashSet<&'a str> {
    let mut bindings = FxHashSet::default();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        if !PROMISES_FS_MODULES.contains(&import.source.value.as_str()) {
            continue;
        }
        let Some(ref specifiers) = import.specifiers else {
            continue;
        };
        for spec in specifiers {
            match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    bindings.insert(def.local.name.as_str());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                    bindings.insert(ns.local.name.as_str());
                }
                ImportDeclarationSpecifier::ImportSpecifier(_) => {}
            }
        }
    }
    bindings
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_fs_read_file() {
        let d = run("import fs from 'fs';\nfs.readFile('f.txt', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fs.promises.readFile"));
    }

    #[test]
    fn flags_fs_write_file() {
        assert_eq!(run("import fs from 'node:fs';\nfs.writeFile('f.txt', data, cb);").len(), 1);
    }

    #[test]
    fn flags_bare_fs_without_import() {
        assert_eq!(run("fs.readFile('f.txt', cb);").len(), 1);
    }

    #[test]
    fn allows_fs_promises() {
        assert!(run("fs.promises.readFile('f.txt');").is_empty());
    }

    #[test]
    fn allows_sync_variant() {
        assert!(run("fs.readFileSync('f.txt');").is_empty());
    }

    #[test]
    fn allows_other_object() {
        assert!(run("myFs.readFile('f.txt', cb);").is_empty());
    }

    // Regression: issue #1100 — `fs` imported from a promise-based module
    // already exposes the promise API, so `fs.method()` must not be flagged.
    #[test]
    fn allows_default_import_from_fs_promises() {
        let src = "import fs from 'fs/promises';\n\
                   const tmpDir = await fs.mkdtemp('dev-tool-check');\n\
                   await fs.writeFile('package.json', '{}');\n\
                   await fs.rm('packPath');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_import_from_node_fs_promises() {
        let src = "import fs from 'node:fs/promises';\n\
                   const raw = await fs.readFile(absolutePath, 'utf8');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_namespace_import_from_fs_promises() {
        let src = "import * as fs from 'node:fs/promises';\n\
                   await fs.writeFile('f.txt', data);";
        assert!(run(src).is_empty());
    }
}
