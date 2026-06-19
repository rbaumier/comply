//! Walks `attribute_item` nodes matching `#[allow(...)]`.
//! Flags when no justification exists: neither an inline `reason = "..."`
//! argument (stabilized in Rust 1.81) nor a `//` comment. For a single-line
//! attribute the comment may sit on the same line, the line above, or the line
//! below; for a multiline attribute it may sit on any line the attribute spans.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("#[allow(") && !text.starts_with("#[allow (") {
        return;
    }

    if has_inline_reason(node, source) {
        return;
    }

    if allow_list_contains(text, "unused") && is_in_test_context(node, source) {
        return;
    }

    let row = node.start_position().row;

    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    if allow_list_contains(text, "dead_code") && has_adjacent_cfg_attribute(&lines, row) {
        return;
    }

    if allow_list_contains(text, "dead_code")
        && ctx.path.components().any(|c| c.as_os_str() == "tests")
    {
        return;
    }

    let has_inline_comment = lines.get(row).is_some_and(|l| {
        if let Some(pos) = l.find("//") {
            pos > l.find("#[allow").unwrap_or(0)
        } else {
            false
        }
    });

    let has_preceding_comment = row > 0 && lines.get(row - 1).is_some_and(|l| l.trim().starts_with("//"));
    let has_following_comment = lines.get(row + 1).is_some_and(|l| l.trim().starts_with("//"));

    let end_row = node.end_position().row;
    let has_inner_comment = end_row > row
        && (row..=end_row).any(|r| lines.get(r).is_some_and(|l| l.contains("//")));

    if has_inline_comment || has_preceding_comment || has_following_comment || has_inner_comment {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{text}` without justification — add a `//` comment explaining why."),
        Severity::Warning,
    ));
}

/// True if the attribute carries an inline `reason = "..."` argument, the
/// justification form stabilized in Rust 1.81 for `#[allow]`/`#[expect]`/
/// `#[warn]`/`#[deny]`. The argument is the justification, so no adjacent
/// `//` comment is required.
///
/// `attribute_item` parses as `attribute_item > attribute > token_tree`, where
/// the token tree holds the comma-separated arguments as a flat sequence of
/// nodes. A `reason` argument appears as the ordered triple `identifier("reason")`,
/// `=`, `string_literal`; detecting that triple in the token tree avoids text
/// scanning, which would also match a lint literally named `reason` or a `reason`
/// substring inside another string.
fn has_inline_reason(attribute_item: Node, source: &[u8]) -> bool {
    let mut item_cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };

    let mut attr_cursor = attribute.walk();
    let Some(token_tree) = attribute
        .children(&mut attr_cursor)
        .find(|child| child.kind() == "token_tree")
    else {
        return false;
    };

    let mut cursor = token_tree.walk();
    let children: Vec<Node> = token_tree.children(&mut cursor).collect();
    children.windows(3).any(|triple| {
        triple[0].kind() == "identifier"
            && triple[0].utf8_text(source) == Ok("reason")
            && triple[1].kind() == "="
            && triple[2].kind() == "string_literal"
    })
}

fn allow_list_contains(attribute: &str, name: &str) -> bool {
    let Some(start) = attribute.find('(') else {
        return false;
    };
    let Some(end) = attribute.rfind(')') else {
        return false;
    };
    attribute[start + 1..end].split(',').any(|part| {
        let candidate = part.trim();
        candidate == name || candidate.ends_with(&format!("::{name}"))
    })
}

fn has_adjacent_cfg_attribute(lines: &[&str], row: usize) -> bool {
    let prev_is_cfg = row > 0
        && lines
            .get(row - 1)
            .is_some_and(|line| line.trim_start().starts_with("#[cfg("));
    let next_is_cfg = lines
        .get(row + 1)
        .is_some_and(|line| line.trim_start().starts_with("#[cfg("));
    prev_is_cfg || next_is_cfg
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_bare_allow() {
        assert_eq!(run("#[allow(dead_code)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_with_inline_comment() {
        assert!(run("#[allow(dead_code)] // kept for FFI compat\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_with_inline_reason_argument() {
        assert!(
            run("#[allow(dead_code, reason = \"deserialized but not directly read\")]\nfn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn allows_with_inline_reason_after_multiple_lints() {
        assert!(
            run("#[allow(unused, clippy::foo, reason = \"kept for symmetry\")]\nfn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn allows_with_multiline_reason_argument() {
        assert!(
            run("#[allow(\n    dead_code,\n    reason = \"kept for symmetry; sizes come \\\n    from FIPS 204 Table 2\"\n)]\nstruct S { priv_key_len: usize }")
                .is_empty()
        );
    }

    #[test]
    fn flags_allow_without_reason_or_comment() {
        // Negative-space guard: a lint named `reason` is not a `reason = "..."`
        // argument and must still be flagged.
        assert_eq!(run("#[allow(dead_code)]\nfn f() {}").len(), 1);
        assert_eq!(run("#[allow(reason)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_with_preceding_comment() {
        assert!(
            run("// mirrors std API naming\n#[allow(clippy::wrong_self_convention)]\nfn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn ignores_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests {\n#[allow(unused)]\nfn f() {}\n}").is_empty());
    }

    #[test]
    fn flags_dead_code_in_test_context_without_reason() {
        assert_eq!(
            run("#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn f() {}\n}").len(),
            1
        );
    }

    #[test]
    fn allows_dead_code_on_cfg_item() {
        assert!(run("#[cfg(feature = \"ffi\")]\n#[allow(dead_code)]\nfn f() {}").is_empty());
        assert!(run("#[allow(dead_code)]\n#[cfg(feature = \"ffi\")]\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_non_allow_attributes() {
        assert!(run("#[derive(Debug)]\nstruct S;").is_empty());
    }

    #[test]
    fn allows_with_following_comment() {
        assert!(run("#[allow(dead_code)]\n// justified below\ntype Foo = i32;").is_empty());
    }

    #[test]
    fn allows_dead_code_in_tests_dir() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "#[allow(dead_code)]\ntype BoxStream<T> = Box<dyn Send>;", "tests/async_send_sync.rs")
        .is_empty());
    }

    #[test]
    fn allows_with_comment_inside_multiline_attribute() {
        // Regression for #3894: the `//` justification lives between the opening
        // `#[allow(` and the closing `)]`, not on an adjacent physical line.
        assert!(
            run("#[repr(transparent)]\n#[allow(\n    unknown_lints,\n    renamed_and_removed_lints,\n    // False positive: https://github.com/rust-lang/rust/issues/115922\n    repr_transparent_non_zst_fields,\n)]\npub struct WithSpan {\n    pub span: Span,\n}")
                .is_empty()
        );
    }

    #[test]
    fn allows_with_inner_comment_simple_multiline() {
        assert!(
            run("#[allow(\n    foo,\n    // because reasons\n    bar,\n)]\nfn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn flags_multiline_allow_without_inner_comment() {
        // Load-bearing guard: a multiline allow with no `//` in its span must
        // still be flagged — the inner scan must not blanket-exempt multiline.
        assert_eq!(run("#[allow(\n    foo,\n    bar,\n)]\nfn f() {}").len(), 1);
    }
}
