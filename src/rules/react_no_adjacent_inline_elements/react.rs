//! react-no-adjacent-inline-elements AST backend.
//!
//! Flags adjacent inline JSX elements (e.g. `<a/><a/>`) that have no
//! whitespace between them.

use crate::diagnostic::{Diagnostic, Severity};

/// Common inline HTML elements.
const INLINE_ELEMENTS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark",
    "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u",
    "var", "wbr", "img", "input", "button", "label", "select", "textarea",
];

fn is_inline_jsx(kind: &str, source: &[u8], node: &tree_sitter::Node) -> bool {
    if kind != "jsx_element" && kind != "jsx_self_closing_element" {
        return false;
    }
    let tag_node = if kind == "jsx_element" {
        let Some(opening) = node.child(0) else {
            return false;
        };
        opening
    } else {
        *node
    };
    let Some(name_node) = tag_node.child_by_field_name("name") else {
        return false;
    };
    let Ok(name) = name_node.utf8_text(source) else {
        return false;
    };
    // User components (PascalCase) are inline by default.
    if name.chars().next().unwrap_or('a').is_ascii_uppercase() {
        return true;
    }
    INLINE_ELEMENTS.contains(&name)
}

crate::ast_check! { on ["jsx_element", "jsx_fragment"] => |node, source, ctx, diagnostics|

    // We look at jsx_element parents and check consecutive children.
    let child_count = node.child_count();
    let mut i = 0;

    while i + 1 < child_count {
        let child_a = node.child(i).unwrap();
        let child_b = node.child(i + 1).unwrap();

        if is_inline_jsx(child_a.kind(), source, &child_a)
            && is_inline_jsx(child_b.kind(), source, &child_b)
        {
            // Check if there is whitespace between them.
            let a_end = child_a.end_byte();
            let b_start = child_b.start_byte();
            let between = &source[a_end..b_start];

            // Only flag if there's NO space at all between them.
            if between.is_empty() {
                let pos = child_b.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "react-no-adjacent-inline-elements".into(),
                    message: "Adjacent inline elements without whitespace — \
                              add `{' '}` or a wrapper."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        i += 1;
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_adjacent_inline_no_space() {
        let src = "const x = <div><span>a</span><span>b</span></div>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_adjacent_with_space() {
        let src = "const x = <div><span>a</span> <span>b</span></div>;";
        assert!(run(src).is_empty());
    }
}
