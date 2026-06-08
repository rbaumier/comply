//! no-xml-external-entity backend for Rust.
//!
//! Flags XML parser usage without XXE protection. In Rust, detects
//! `ParserConfig::new()` (xml-rs) and `Reader::from_*` (quick-xml)
//! without explicit feature restrictions.

use crate::diagnostic::{Diagnostic, Severity};

const XML_PARSER_PATTERNS: &[&str] = &[
    "ParserConfig",
    "EventReader::new",
    "XmlReader::new",
    "from_reader",
    "from_str",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    // Check context: only flag if line references XML parsing
    let line_idx = node.start_position().row;
    let full_text = std::str::from_utf8(source).unwrap_or("");
    let line = match full_text.lines().nth(line_idx) {
        Some(l) => l,
        None => return,
    };
    let line_lower = line.to_ascii_lowercase();

    // Must be in an XML context
    if !line_lower.contains("xml") && !callee_text.contains("Xml") && !callee_text.contains("xml") {
        return;
    }

    for &pattern in XML_PARSER_PATTERNS {
        if callee_text.contains(pattern) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-xml-external-entity".into(),
                message: "XML parser without explicit XXE protection — disable external entity resolution.".into(),
                severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_xml_event_reader() {
        assert_eq!(
            run_on("fn f() { let parser = xml::EventReader::new(input); }").len(),
            1,
        );
    }

    #[test]
    fn flags_xml_parser_config() {
        assert_eq!(
            run_on("fn f() { let config = xml::ParserConfig::new(); }").len(),
            1,
        );
    }

    #[test]
    fn allows_non_xml_reader() {
        assert!(run_on("fn f() { let r = BufReader::new(file); }").is_empty());
    }
}
