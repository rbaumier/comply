use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const FS_METHODS: &[&str] = &[
    "readFile",
    "readFileSync",
    "writeFile",
    "writeFileSync",
    "unlink",
    "unlinkSync",
    "createReadStream",
    "createWriteStream",
    "appendFile",
    "appendFileSync",
];

const USER_DATA_NEEDLES: &[&str] = &[
    "req.params",
    "req.query",
    "req.body",
    "request.params",
    "request.query",
    "request.body",
    "searchParams.get",
    "params.",
];

const SANITIZER_NEEDLES: &[&str] = &["basename(", "path.resolve(", "normalize("];

fn arg_is_user_controlled(text: &str) -> bool {
    if SANITIZER_NEEDLES.iter().any(|s| text.contains(s)) {
        return false;
    }
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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

        // Callee must be `fs.method(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !FS_METHODS.contains(&method) {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "fs" {
            return;
        }

        // Check first argument for user-controlled data.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let span = first_arg.span();
        let arg_text = ctx
            .source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("");
        if !arg_is_user_controlled(arg_text) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "User-controlled path in `fs` call \u{2014} use `path.basename()` or validate against a safe root."
                    .into(),
            severity: Severity::Error,
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
    fn flags_fs_read_with_req_params() {
        assert_eq!(run_on("fs.readFile(req.params.filename)").len(), 1);
    }


    #[test]
    fn flags_write_with_query() {
        assert_eq!(run_on("fs.writeFile(req.query.path, data)").len(), 1);
    }


    #[test]
    fn allows_basename_sanitization() {
        assert!(run_on("fs.readFile(path.basename(req.params.filename))").is_empty());
    }


    #[test]
    fn allows_literal_path() {
        assert!(run_on("fs.readFile('/data/file.txt')").is_empty());
    }
}
