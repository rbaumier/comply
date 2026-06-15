//! no-suspicious-semicolon-in-jsx oxc backend.
//!
//! Flags a JSX text child whose source begins with `;` immediately followed
//! by a line break. That shape only occurs when a semicolon sits directly
//! after a closing or self-closing tag (`<div />;\n`), which React renders as
//! a literal `;` — almost always a typo. A semicolon on its own line, inside
//! an expression container (`{';'}`), encoded as an entity (`&#59;`), or
//! preceded by other text does not match.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // JSXText has no dispatchable AstType; iterate via run_on_semantic.
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // `JSXText.value` is the parsed value (entities decoded); inspect the
        // raw source slice so `&#59;` stays distinct from a literal `;`.
        for node in semantic.nodes().iter() {
            if let AstKind::JSXText(text) = node.kind() {
                let raw = source_slice(ctx.source, text.span.start, text.span.end);
                if raw.starts_with(";\n") || raw.starts_with(";\r") {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, text.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Suspicious semicolon in JSX — it is rendered as \
                                  literal text. Remove it or move it inside `{';'}`."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

fn source_slice(source: &str, start: u32, end: u32) -> &str {
    let s = start as usize;
    let e = (end as usize).min(source.len());
    if s >= e {
        return "";
    }
    &source[s..e]
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    // --- Invalid (Biome invalid.jsx fixtures) ---

    #[test]
    fn flags_semicolon_after_self_closing_in_element() {
        let src = "const Component = () => {\n    return (\n        <div>\n          <div />;\n        </div>\n    );\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_semicolon_after_closing_tag_in_element() {
        let src = "const Component2 = () => {\n    return (\n        <div>\n          <Component>\n            <div />\n          </Component>;\n        </div>\n    );\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_semicolon_in_arrow_implicit_return() {
        let src = "const Component3 = () => (\n    <div>\n        <Component />;\n    </div>\n)";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_semicolon_after_self_closing_in_fragment() {
        let src = "const Component4 = () => {\n  return (\n      <>\n          <div />;\n      </>\n  );\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_semicolon_after_closing_tag_in_fragment() {
        let src = "const Component5 = () => {\n  return (\n      <>\n        <Component>\n          <div />\n        </Component>;\n      </>\n  );\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_semicolon_with_crlf() {
        let src = "const c = () => {\r\n  return (\r\n    <div>\r\n      <div />;\r\n    </div>\r\n  );\r\n}";
        assert_eq!(run(src).len(), 1);
    }

    // --- Valid (Biome valid.jsx fixtures) ---

    #[test]
    fn allows_no_semicolon() {
        let src = "const Component = () => {\n    return (\n        <div>\n            <div />\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_semicolon_on_its_own_line() {
        let src = "const Component2 = () => {\n    return (\n        <div>\n            <div />\n            ;\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_semicolon_in_expression_container() {
        let src = "const Component3 = () => {\n    return (\n        <div>\n          <div />{';'}\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_semicolon_as_entity() {
        let src = "const Component4 = () => {\n    return (\n        <div>\n          <div />&#59;\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_semicolons_and_text() {
        let src = "const Component5 = () => {\n    return (\n        <div>\n          <span>;</span>\n          <span />;<span />\n          text; text;\n          &amp;\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_jsx_in_statement_position() {
        let src = "const Component6 = () => {\n    return <div />;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_text_then_semicolon() {
        let src = "const Component7 = () => {\n    return (\n        <div>\n            <div />text;\n        </div>\n    );\n}";
        assert!(run(src).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn allows_semicolon_inside_string_literal_child() {
        let src = "const c = () => <div>{\";\\nstill a string\"}</div>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_semicolon_followed_by_space_not_newline() {
        let src = "const c = () => (\n    <div>\n        <span />; <span />\n    </div>\n);";
        assert!(run(src).is_empty());
    }
}
