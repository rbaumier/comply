use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fs"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();

        // Skip Sync variants — handled by node-no-sync.
        if method.ends_with("Sync") {
            return;
        }
        if !FS_METHODS.contains(&method) {
            return;
        }

        // Object must be the bare `fs` identifier (not `fs.promises`).
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "fs" {
            return;
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
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_fs_read_file() {
        let d = run_on("fs.readFile('f.txt', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fs.promises.readFile"));
    }


    #[test]
    fn flags_fs_write_file() {
        assert_eq!(run_on("fs.writeFile('f.txt', data, cb);").len(), 1);
    }


    #[test]
    fn allows_fs_promises() {
        assert!(run_on("fs.promises.readFile('f.txt');").is_empty());
    }


    #[test]
    fn allows_sync_variant() {
        assert!(run_on("fs.readFileSync('f.txt');").is_empty());
    }


    #[test]
    fn allows_other_object() {
        assert!(run_on("myFs.readFile('f.txt', cb);").is_empty());
    }
}
