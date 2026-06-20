//! no-xml-external-entity backend for Rust.
//!
//! Flags XML parser usage without XXE protection. In Rust, detects xml-rs
//! (`ParserConfig`, `EventReader`, `XmlReader`) and other external-entity-capable
//! deserializers in an XML context. quick_xml is exempt: it is a streaming parser
//! with no external-entity expansion, so XXE is impossible by construction.
//!
//! The XXE risk lives in an *application* that hands untrusted XML to a parser
//! as a production sink. Two exemptions:
//! - test and cargo-fuzz files (any crate): a parser fed XML in a `#[test]` or a
//!   fuzz target is exercising the parser on fixtures, not a production sink;
//! - the source of a known XML-parsing crate (`quick-xml`, `xml-rs`,
//!   `roxmltree`, …): the library implementing XML parsing is never the
//!   downstream consumer mis-using it — see
//!   [`crate::project::CargoManifest::is_xml_parser_crate`].

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

    // Test code and cargo-fuzz targets (any crate) feed XML to a parser to
    // exercise it on fixtures, not as a production sink — XXE in a fixture is
    // not a runtime vulnerability.
    // Dual-read: the unit-test harness injects an empty default FileCtx, so the
    // `path_segments` flags are false in tests — fall back to the pure path
    // predicates, which read `ctx.path` directly.
    if crate::rules::rust_helpers::is_in_test_context(node, source)
        || crate::rules::rust_helpers::is_under_tests_dir(ctx.path)
        || ctx.file.path_segments.in_test_dir
        || ctx.file.path_segments.in_fuzz_targets
        || crate::rules::path_utils::is_fuzz_targets_path(ctx.path)
    {
        return;
    }
    // The crate under analysis IS an XML-parsing library: its source implements
    // `from_str`/`*Reader::new` rather than consuming them, so it is never the
    // downstream application that could mis-use a parser.
    if ctx
        .project
        .nearest_cargo_manifest(ctx.path)
        .is_some_and(|m| m.is_xml_parser_crate())
    {
        return;
    }

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
    use std::fs;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    /// Run on `rel_path` inside a temp crate with the given `Cargo.toml`, so the
    /// `is_xml_parser_crate` check resolves against a controlled manifest instead
    /// of comply's own.
    fn run_in_crate(cargo_toml_contents: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        let src_path = dir.path().join(rel_path);
        if let Some(parent) = src_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
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

    const QUICK_XML_CARGO_TOML: &str = r#"
[package]
name = "quick-xml"
version = "0.31.0"
edition = "2021"

[lib]
name = "quick_xml"
path = "src/lib.rs"
"#;

    #[test]
    fn allows_quick_xml_own_source_issue4928() {
        // Issue #4928: quick-xml's own Deserializer constructor wires its own
        // `XmlReader::new` — the library implementing XML parsing is never the
        // downstream consumer that could mis-use it.
        assert!(
            run_in_crate(
                QUICK_XML_CARGO_TOML,
                "src/de/mod.rs",
                "fn new(reader: R) -> Self { Self { reader: XmlReader::new(reader) } }",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_xml_library_tests_issue4928() {
        // Issue #4928: quick-xml's own namespace test exercises `NsReader::from_str`
        // on a fixture by design. A `tests/` integration file is test code.
        assert!(
            run_on_path(
                "fn namespace() { let mut r = NsReader::from_str(\"<a xmlns:myns='www1'></a>\"); }",
                "tests/reader-namespaces.rs",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_xml_library_fuzz_target_issue4928() {
        // Issue #4928: a cargo-fuzz target feeds the parser fuzzed XML by design.
        assert!(
            run_on_path(
                "fn fuzz(data: &[u8]) { let _ = Reader::from_str(s); }",
                "fuzz/fuzz_targets/structured_roundtrip.rs",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_xxe_in_application_consuming_xml_parser_issue4928() {
        // The library-self exemption must not leak to a downstream application:
        // an ordinary (non-XML-library, non-test) crate that hands untrusted XML
        // to xml-rs is still flagged.
        const APP_CARGO_TOML: &str = r#"
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "myapp"
path = "src/main.rs"
"#;
        assert_eq!(
            run_in_crate(
                APP_CARGO_TOML,
                "src/handler.rs",
                "fn parse(input: &str) { let p = xml::EventReader::new(input); }",
            )
            .len(),
            1,
        );
    }
}
