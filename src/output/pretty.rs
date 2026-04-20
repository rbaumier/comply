//! Miette-powered pretty renderer. Groups diagnostics by file, reads each
//! source file once, and emits a labeled source frame per diagnostic with
//! rule help and doc URL pulled from the RuleMeta registry.
//!
//! Fallbacks:
//! - Diagnostic with `span: None`: uses `span_resolver::resolve_line_span`
//!   to highlight the full line at the reported `(line, column)`.
//! - File unreadable (race, virtual path, deleted between scan and render):
//!   falls back to the eslint-like single line for that diagnostic — no
//!   crash, no error.
//! - `rule_id` absent from the `RuleMeta` registry (delegated oxlint/clippy
//!   diagnostics): help and url omitted; frame still rendered.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::meta::RuleMeta;
use crate::rules::meta_registry;
use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, SourceSpan};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::span_resolver::resolve_line_span;

/// Render a slice of diagnostics in human-pretty format using miette. Groups
/// by file path so each file's source is read exactly once. Diagnostics whose
/// file can't be read fall back to the eslint-like single-line form for that
/// entry only.
#[must_use]
pub fn render_pretty(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    // `without_primary_span_start()` stops miette from appending its own
    // `:{line}:{col}` suffix to the header label. We then own the whole
    // content between the brackets via `NamedSource.name()` — letting us
    // pad with spaces on both sides (`[ path:line:col ]`) so iTerm /
    // wezterm / kitty Smart Selection doesn't pick up the adjacent
    // `╭─` arc or the `]` when cmd+click resolves the path.
    let handler = GraphicalReportHandler::new()
        .with_theme(GraphicalTheme::unicode())
        .without_primary_span_start();

    // BTreeMap → stable, alphabetical file ordering for reproducible output.
    // Within a file we preserve the caller's original order.
    let mut by_file: BTreeMap<&std::path::Path, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diagnostics {
        by_file.entry(d.path.as_path()).or_default().push(d);
    }

    for (path, diags) in by_file {
        let source_text = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                // File unreadable — fall back to eslint line per diagnostic.
                for d in &diags {
                    super::write_eslint_line(&mut out, d);
                }
                continue;
            }
        };
        let source_arc = Arc::new(source_text);

        for d in diags {
            let span_pair = d
                .span
                .or_else(|| resolve_line_span(source_arc.as_str(), d.line, d.column))
                .unwrap_or((0, 0));
            // Padding on both sides isolates `path:line:col` from the
            // surrounding `[`, `]`, and leading `╭─` arc, so Smart
            // Selection in iTerm / wezterm / kitty extracts only the
            // path on cmd+click.
            let name = format!(" {}:{}:{} ", path.display(), d.line, d.column);
            let md = MietteDiag {
                diag: d,
                meta: meta_registry::lookup(&d.rule_id),
                source: NamedSource::new(name, Arc::clone(&source_arc)),
                span: SourceSpan::new(span_pair.0.into(), span_pair.1),
            };
            // Writing into a String never fails; `expect` here is safe.
            handler
                .render_report(&mut out, &md)
                .expect("render_report into String cannot fail");
            out.push('\n');
        }
    }

    out
}

// Thin wrapper so we can hand one Diagnostic at a time to miette's
// GraphicalReportHandler.
// One MietteDiag per diagnostic; the shared Arc<String> makes the source handle cheap to copy.
struct MietteDiag<'a> {
    diag: &'a Diagnostic,
    meta: Option<RuleMeta>,
    source: NamedSource<Arc<String>>,
    span: SourceSpan,
}

impl std::fmt::Debug for MietteDiag<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MietteDiag")
            .field("rule_id", &self.diag.rule_id)
            .finish()
    }
}

impl std::fmt::Display for MietteDiag<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.diag.message)
    }
}

impl std::error::Error for MietteDiag<'_> {}

impl miette::Diagnostic for MietteDiag<'_> {
    fn code<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        Some(Box::new(self.diag.rule_id.as_str()))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.diag.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
        })
    }

    fn help<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta
            .as_ref()
            .map(|m| Box::new(m.remediation) as Box<dyn std::fmt::Display + 'b>)
    }

    fn url<'b>(&'b self) -> Option<Box<dyn std::fmt::Display + 'b>> {
        self.meta
            .and_then(|m| m.doc_url)
            .map(|u| Box::new(u) as Box<dyn std::fmt::Display + 'b>)
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source)
    }

    // Label text is omitted — miette draws the message as the error header already; a label would repeat it next to the caret.
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            None,
            self.span,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::path::PathBuf;

    fn write_fixture(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-miette-pretty-tests");
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn renders_rule_id_message_and_source_frame() {
        let path = write_fixture("fixture_a.ts", "const x = 1;\n");
        let diag = Diagnostic {
            path: path.clone(),
            line: 1,
            column: 7,
            rule_id: "no-weak-cipher".into(), // real rule id in registry
            message: "example message".into(),
            severity: Severity::Warning,
            span: Some((6, 1)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("no-weak-cipher"), "rule id missing: {out}");
        assert!(out.contains("example message"), "message missing: {out}");
        assert!(out.contains("const x = 1;"), "source frame missing: {out}");
    }

    #[test]
    fn unreadable_file_falls_back_to_eslint_line() {
        let path = PathBuf::from("/definitely/does/not/exist/xyz/foo.ts");
        let diag = Diagnostic {
            path: path.clone(),
            line: 10,
            column: 5,
            rule_id: "no-weak-cipher".into(),
            message: "msg".into(),
            severity: Severity::Error,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(
            out.contains("foo.ts:10:5: error [no-weak-cipher] msg"),
            "unreadable-file fallback missing: {out}"
        );
    }

    #[test]
    fn unknown_rule_id_still_renders_frame_without_help() {
        let path = write_fixture("fixture_b.ts", "abc\n");
        let diag = Diagnostic {
            path,
            line: 1,
            column: 1,
            rule_id: "not-a-real-rule-id".into(),
            message: "unknown rule message".into(),
            severity: Severity::Warning,
            span: Some((0, 3)),
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("not-a-real-rule-id"));
        assert!(out.contains("unknown rule message"));
    }

    #[test]
    fn diag_without_span_resolves_whole_line() {
        let path = write_fixture("fixture_c.ts", "first\nsecond\n");
        let diag = Diagnostic {
            path,
            line: 2,
            column: 1,
            rule_id: "no-weak-cipher".into(),
            message: "m".into(),
            severity: Severity::Warning,
            span: None,
        };
        let out = render_pretty(&[diag]);
        assert!(out.contains("second"), "second line not highlighted: {out}");
    }
}
