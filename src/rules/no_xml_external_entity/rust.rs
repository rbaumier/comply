//! no-xml-external-entity backend for Rust.
//!
//! Flags XML parser usage without XXE protection. In Rust, detects xml-rs
//! (`ParserConfig`, `EventReader`, `XmlReader`) and other external-entity-capable
//! deserializers in an XML context. quick_xml is exempt: it is a streaming parser
//! with no external-entity expansion, so XXE is impossible by construction.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if any descendant of `node` is a `call_expression`.
///
/// Used to distinguish an inner parser call from a chained outer call: the outer
/// call's callee subtree (e.g. the `field_expression` `inner(..).map_err`) embeds
/// the inner `call_expression`, whereas the inner call's callee is a bare path.
fn callee_subtree_contains_call_expression(node: tree_sitter::Node) -> bool {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        for i in 0..current.child_count() {
            let Some(child) = current.child(i) else {
                continue;
            };
            if child.kind() == "call_expression" {
                return true;
            }
            stack.push(child);
        }
    }
    false
}

const XML_PARSER_PATTERNS: &[&str] = &[
    "ParserConfig",
    "EventReader::new",
    "XmlReader::new",
    "from_reader",
    "from_str",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    // A chained outer call (`inner(...).map_err(...)`) has a callee subtree that
    // contains the inner call, so a substring pattern match would re-flag the same
    // construction. Only the innermost call is the real parser usage — skip any call
    // whose callee itself contains a nested call_expression.
    if callee_subtree_contains_call_expression(callee) {
        return;
    }

    let callee_text = callee.utf8_text(source).unwrap_or("");

    // quick_xml is a streaming parser with no external-entity expansion — XXE is
    // impossible by construction, so it is never flagged.
    if callee_text.contains("quick_xml") {
        return;
    }

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

    #[test]
    fn allows_quick_xml_chained_repro() {
        assert!(run_on(
            "fn f() { let _ = quick_xml::de::from_reader(body.take()?.into_bytes().as_ref()).map_err(E::Parse)?; }"
        )
        .is_empty());
    }

    #[test]
    fn allows_quick_xml_simple() {
        assert!(run_on("fn f() { let x = quick_xml::de::from_reader(r); }").is_empty());
    }

    #[test]
    fn flags_chained_xml_parser_once() {
        assert_eq!(
            run_on("fn f() { let _ = xml::EventReader::new(input).next(); }").len(),
            1,
        );
    }

    #[test]
    fn flags_parser_with_call_argument() {
        // The argument is a call, but it lives in the `arguments` field, not
        // `function` — the chained-call guard must not skip the parser itself.
        assert_eq!(
            run_on("fn f() { let p = xml::EventReader::new(get_input()); }").len(),
            1,
        );
    }

    #[test]
    fn flags_serde_xml_rs() {
        // serde-xml-rs is built on xml-rs and IS XXE-capable — the quick_xml
        // exemption must not extend to other XML deserializers.
        assert_eq!(
            run_on("fn f() { let v: T = serde_xml_rs::from_str(xml_input).unwrap(); }").len(),
            1,
        );
    }
}
