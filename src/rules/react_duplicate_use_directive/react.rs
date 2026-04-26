//! react-duplicate-use-directive backend.
//!
//! Fires once per file, on the `program` root node, when `ctx.file.directives`
//! captures both flags. The directive scanner only records top-level string
//! expression statements, so inline `"use server"` inside functions doesn't
//! false-positive.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    if !(ctx.file.directives.use_client && ctx.file.directives.use_server) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-duplicate-use-directive".into(),
        message: "This file has both `\"use client\"` and `\"use server\"`. \
                  Only the first directive takes effect — pick one."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileDirectives, FileCtx};

    fn both_directives() -> FileCtx {
        FileCtx {
            directives: FileDirectives {
                use_client: true,
                use_server: true,
            },
            ..Default::default()
        }
    }

    fn client_only() -> FileCtx {
        FileCtx {
            directives: FileDirectives {
                use_client: true,
                use_server: false,
            },
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_file_ctx(source, &Check, file)
    }

    #[test]
    fn flags_both_directives() {
        let src = r#"
"use client";
"use server";
export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &both_directives()).len(), 1);
    }

    #[test]
    fn allows_only_use_client() {
        let src = r#"
"use client";
export default function Page() { return <div />; }
"#;
        assert!(run(src, &client_only()).is_empty());
    }

    #[test]
    fn allows_neither_directive() {
        let src = r#"
export default function Page() { return <div />; }
"#;
        assert!(run(src, &FileCtx::default()).is_empty());
    }
}
