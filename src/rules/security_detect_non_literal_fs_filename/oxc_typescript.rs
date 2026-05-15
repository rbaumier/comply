//! security-detect-non-literal-fs-filename oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// fs.* methods that take a path as first argument.
const FS_PATH_METHODS: &[&str] = &[
    "readFile",
    "readFileSync",
    "writeFile",
    "writeFileSync",
    "appendFile",
    "appendFileSync",
    "open",
    "openSync",
    "rm",
    "rmSync",
    "unlink",
    "unlinkSync",
    "stat",
    "statSync",
    "lstat",
    "lstatSync",
    "access",
    "accessSync",
    "createReadStream",
    "createWriteStream",
    "readdir",
    "readdirSync",
    "mkdir",
    "mkdirSync",
    "rmdir",
    "rmdirSync",
    "copyFile",
    "copyFileSync",
    "rename",
    "renameSync",
    "exists",
    "existsSync",
];

fn callee_uses_fs(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let method = member.property.name.as_str();
    if !FS_PATH_METHODS.contains(&method) {
        return false;
    }
    let receiver_name = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    receiver_name == "fs" || receiver_name == "fsPromises" || receiver_name == "fsp"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fs."])
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
        if !callee_uses_fs(call) {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        let is_literal = match expr {
            Expression::StringLiteral(_) => true,
            Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
            _ => false,
        };
        if is_literal {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Dynamic path passed to `fs.*` — path traversal vector when the \
                      input is user-controlled. Validate against an allowlist."
                .into(),
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
    fn flags_fs_read_dynamic() {
        let src = r#"const r = fs.readFileSync(userInput);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_fs_read_literal() {
        let src = r#"const r = fs.readFileSync("config.json");"#;
        assert!(run(src).is_empty());
    }
}
