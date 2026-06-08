//! prefer-blob-reading-methods backend — flag `FileReader#readAsText` / `readAsArrayBuffer`.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[(&str, &str)] = &[("readAsText", "text"), ("readAsArrayBuffer", "arrayBuffer")];

crate::ast_check! { on ["call_expression"] prefilter = ["readAsText", "readAsArrayBuffer"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    for &(method, replacement) in METHODS {
        if prop_name == method {
            let pos = prop.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-blob-reading-methods".into(),
                message: format!(
                    "Prefer `Blob#{}()` over `FileReader#{}(blob)`.",
                    replacement, method
                ),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
    }
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_read_as_text() {
        let d = run_on("reader.readAsText(blob);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Blob#text()"));
    }

    #[test]
    fn flags_read_as_array_buffer() {
        let d = run_on("reader.readAsArrayBuffer(blob);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Blob#arrayBuffer()"));
    }

    #[test]
    fn allows_blob_text() {
        assert!(run_on("const text = await blob.text();").is_empty());
    }

    #[test]
    fn allows_unrelated_code() {
        assert!(run_on("const data = JSON.parse(response);").is_empty());
    }
}
