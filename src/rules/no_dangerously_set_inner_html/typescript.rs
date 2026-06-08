//! no-dangerously-set-inner-html backend — flag React's
//! `dangerouslySetInnerHTML` prop.
//!
//! Why: the API is called "dangerously" for a reason. Any user-controlled
//! HTML passed through it becomes an XSS vector. If you genuinely need to
//! render HTML (from a CMS, markdown, etc.), sanitize via DOMPurify first
//! and add a code comment explaining the provenance — but the default
//! answer is "don't".

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["dangerouslySetInnerHTML"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else {
        return;
    };
    if name != "dangerouslySetInnerHTML" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-dangerously-set-inner-html".into(),
        message: "`dangerouslySetInnerHTML` is an XSS vector. If you must \
                  render user-facing HTML, sanitize it with DOMPurify first \
                  and add a comment explaining the content's provenance."
            .into(),
        severity: Severity::Error,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_dangerously_set_inner_html() {
        let source = "const x = <div dangerouslySetInnerHTML={{ __html: raw }} />;";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_regular_jsx_attributes() {
        assert!(run_on("const x = <div className='foo'>text</div>;").is_empty());
    }
}
