use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const METHODS: &[(&str, &str)] = &[("readAsText", "text"), ("readAsArrayBuffer", "arrayBuffer")];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["readAsText", "readAsArrayBuffer"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression
        let Expression::StaticMemberExpression(member) = &call.callee else { return };

        let prop_name = member.property.name.as_str();

        for &(method, replacement) in METHODS {
            if prop_name == method {
                let (line, column) = byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
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
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
