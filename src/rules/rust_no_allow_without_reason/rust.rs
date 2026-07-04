//! Walks `attribute_item` nodes matching `#[allow(...)]`.
//! Flags when no justification exists: neither an inline `reason = "..."`
//! argument (stabilized in Rust 1.81) nor a `//` comment. For a single-line
//! attribute the comment may sit on the same line, the line above, or the line
//! below — and on the line below it counts whether it is a standalone comment
//! or a trailing inline comment on the attributed item's code. For a multiline
//! attribute it may sit on any line the attribute spans.
//! A comment above the first of a consecutive `#[allow]` cluster justifies every
//! member of the cluster.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    enclosing_fn, has_outer_attribute, has_test_attribute, is_in_test_context,
};

/// Deprecated methods of the `std::error::Error` trait. Implementing one of
/// these on a wrapper type forces a delegating call to the inner type's
/// same-name deprecated method, which is what `#[allow(deprecated)]` suppresses
/// — the deprecated context is the justification.
const DEPRECATED_TRAIT_METHODS: &[&str] = &["description", "cause"];

/// Lints whose suppression names its own reason, so a separate `//` comment or
/// `reason = "..."` would only restate the lint:
/// - The `nonstandard_style` naming-convention family (`nonstandard_style`
///   itself and its members `non_upper_case_globals`, `non_camel_case_types`,
///   `non_snake_case`): the item's name deliberately mirrors an external
///   identifier — IANA timezone names like `Africa__Abidjan`, or C/FFI type
///   names like `VkFooBar`/`size_t` defined by a foreign spec — and cannot be
///   renamed to Rust's casing without losing the mapping. comply already honors
///   this same opt-out in `screaming-snake-for-constants`.
/// - `missing_docs`: suppressing the missing-documentation lint *is* the
///   statement that the item is intentionally undocumented.
///
/// These are stylistic-convention lints, not correctness/safety concerns
/// (`dead_code`, `unused`, `deprecated`), which still require a justification.
const SELF_JUSTIFYING_LINTS: &[&str] = &[
    "non_upper_case_globals",
    "non_camel_case_types",
    "non_snake_case",
    "nonstandard_style",
    "missing_docs",
];

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    if !is_allow_attribute(text) {
        return;
    }

    if has_inline_reason(node, source) {
        return;
    }

    if all_lints_self_justifying(text) {
        return;
    }

    if (allow_list_contains(text, "unused")
        || allow_list_contains(text, "deprecated")
        || allow_list_has_clippy_lint(text))
        && is_test_scoped(node, source)
    {
        return;
    }

    if allow_list_contains(text, "deprecated") && is_in_deprecated_context(node, source) {
        return;
    }

    let row = node.start_position().row;

    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    if allow_list_contains(text, "dead_code") && attribute_stack_has_cfg(node, source) {
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

    let has_preceding_comment = cluster_has_preceding_comment(node, source);
    let has_following_comment = lines.get(row + 1).is_some_and(|l| line_has_comment(l));

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

/// True if an `attribute_item`'s text is an `#[allow(...)]` attribute, with or
/// without a space before the argument list.
fn is_allow_attribute(text: &str) -> bool {
    text.starts_with("#[allow(") || text.starts_with("#[allow (")
}

/// True when the flagged `#[allow(...)]` belongs to a consecutive cluster of
/// `#[allow]` attributes whose first member is immediately preceded by a `//`
/// (or `/* */`) comment — the comment documents the whole stacked cluster, so
/// every member is justified.
///
/// Outer attributes and comments on an item are named siblings preceding it in
/// tree-sitter-rust. Walking `prev_named_sibling` from the flagged attribute,
/// each step must be physically adjacent — the sibling ends on the line directly
/// above where the current cluster member begins. A blank line, or any
/// intervening node that is not another `#[allow]`, ends the cluster. So a lone
/// uncommented `#[allow]`, or one separated from the comment by a blank line or
/// unrelated code, is not justified by this path.
fn cluster_has_preceding_comment(node: Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(prev) = current.prev_named_sibling() {
        if prev.end_position().row + 1 != current.start_position().row {
            return false;
        }
        match prev.kind() {
            "line_comment" | "block_comment" => return true,
            "attribute_item" if is_allow_attribute(prev.utf8_text(source).unwrap_or("")) => {
                current = prev;
            }
            _ => return false,
        }
    }
    false
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

/// True when an `#[allow(deprecated)]` is self-justified by its enclosing
/// function: either the function carries its own `#[deprecated]` attribute, or
/// its name is a deprecated standard trait method whose implementation must
/// delegate to the inner type's deprecated method.
///
/// In both cases the deprecation *is* the reason — a delegating implementation
/// of deprecated code necessarily touches deprecated APIs — so an extra `//`
/// comment would only restate what the surrounding code already shows.
fn is_in_deprecated_context(node: Node, source: &[u8]) -> bool {
    let Some(func) = enclosing_fn(node) else {
        return false;
    };

    if has_outer_attribute(func, source, "deprecated") {
        return true;
    }

    func.child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .is_some_and(|name| DEPRECATED_TRAIT_METHODS.contains(&name))
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

/// True when the allow list names at least one `clippy::`-prefixed lint
/// (`clippy::reversed_empty_ranges`, `clippy::bool_assert_comparison`, …). The
/// discriminator is the `clippy::` lint *namespace*, not any single lint name.
fn allow_list_has_clippy_lint(attribute: &str) -> bool {
    let Some(start) = attribute.find('(') else {
        return false;
    };
    let Some(end) = attribute.rfind(')') else {
        return false;
    };
    attribute[start + 1..end]
        .split(',')
        .any(|part| part.trim().starts_with("clippy::"))
}

/// True when the `#[allow(...)]` sits in test code: either inside an enclosing
/// `#[test]`/`#[cfg(test)]` function, module, or impl ([`is_in_test_context`]),
/// or as an outer attribute *decorating* a `#[test]`-attributed item
/// ([`decorates_test_item`]). The latter covers an `#[allow]` stacked alongside
/// `#[test]` on the same function — there the `#[test]` is a sibling attribute,
/// not an ancestor, so the ancestor walk alone misses it.
fn is_test_scoped(node: Node, source: &[u8]) -> bool {
    is_in_test_context(node, source) || decorates_test_item(node, source)
}

/// True when this `attribute_item` decorates an item that itself carries a
/// `#[test]` / `#[cfg(test)]` attribute — i.e. the `#[allow]` is part of the
/// outer-attribute run of a test item.
///
/// tree-sitter-rust models outer attributes as `attribute_item` siblings
/// preceding the item they decorate, so the decorated item is the first
/// following named sibling that is neither another attribute nor a comment.
/// [`has_test_attribute`] then scans that item's own preceding attributes for a
/// test marker, regardless of the `#[test]`/`#[allow]` ordering.
fn decorates_test_item(attribute_item: Node, source: &[u8]) -> bool {
    let mut sibling = attribute_item.next_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" | "line_comment" | "block_comment" => {
                sibling = s.next_named_sibling();
            }
            _ => return has_test_attribute(s, source),
        }
    }
    false
}

/// True when the allow attribute names at least one lint and *every* lint it
/// names is in [`SELF_JUSTIFYING_LINTS`]. A mixed list (e.g.
/// `#[allow(missing_docs, dead_code)]`) is not exempt, since `dead_code` still
/// requires a justification.
fn all_lints_self_justifying(attribute: &str) -> bool {
    let Some(start) = attribute.find('(') else {
        return false;
    };
    let Some(end) = attribute.rfind(')') else {
        return false;
    };
    let mut saw_lint = false;
    for part in attribute[start + 1..end].split(',') {
        let candidate = part.trim();
        if candidate.is_empty() {
            continue;
        }
        saw_lint = true;
        if !SELF_JUSTIFYING_LINTS.contains(&candidate) {
            return false;
        }
    }
    saw_lint
}

/// True when the `#[allow(...)]` item's attribute stack contains a `#[cfg(...)]`
/// attribute anywhere, in either direction. A `#[cfg(...)]` gate can make the
/// decorated item unreachable in some build configurations, which is the
/// standing justification for an accompanying `#[allow(dead_code)]` — even when
/// another attribute (`#[inline]`, `#[must_use]`, `#[cold]`, …) sits between the
/// cfg and the allow.
///
/// tree-sitter-rust models outer attributes as `attribute_item` siblings of the
/// item they decorate. Walking `prev_named_sibling`/`next_named_sibling` over the
/// contiguous run of `attribute_item` siblings and stopping at the first
/// non-`attribute_item` sibling keeps the check scoped to this item: the first
/// non-attribute sibling is the decorated item (or unrelated code), so a
/// `#[cfg(...)]` decorating a *different* item cannot leak across that boundary.
fn attribute_stack_has_cfg(node: Node, source: &[u8]) -> bool {
    let mut prev = node.prev_named_sibling();
    while let Some(sibling) = prev {
        if sibling.kind() != "attribute_item" {
            break;
        }
        if is_cfg_attribute(sibling, source) {
            return true;
        }
        prev = sibling.prev_named_sibling();
    }

    let mut next = node.next_named_sibling();
    while let Some(sibling) = next {
        if sibling.kind() != "attribute_item" {
            break;
        }
        if is_cfg_attribute(sibling, source) {
            return true;
        }
        next = sibling.next_named_sibling();
    }

    false
}

/// True when `attribute_item` is a `#[cfg(...)]` attribute. `#[cfg_attr(...)]` is
/// excluded: it conditionally *applies* an attribute rather than gating the
/// item's presence, so it does not justify a dead-code allow. Uses the rule's
/// text idiom for attribute recognition.
fn is_cfg_attribute(attribute_item: Node, source: &[u8]) -> bool {
    attribute_item
        .utf8_text(source)
        .unwrap_or("")
        .trim_start()
        .starts_with("#[cfg(")
}

/// True when `line` carries a `//` line comment — either a standalone comment or
/// a trailing inline comment on a code line. A `//` inside a double-quoted string
/// literal (e.g. `"a//b"`) is skipped so it is not mistaken for a comment; this
/// mirrors how the same-line check treats `//` after the `#[allow(...)]` text as
/// a justification. Only double-quoted string state is tracked (with `\` escapes);
/// char literals and raw-string hashes are not modeled — an accepted edge for a
/// trailing-comment check.
fn line_has_comment(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => {
                i += 2; // skip the escaped character, e.g. `\"`
                continue;
            }
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes.get(i + 1) == Some(&b'/') => return true,
            _ => {}
        }
        i += 1;
    }
    false
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
    fn ignores_clippy_on_test_fn() {
        // #6197: an `#[allow(clippy::*)]` stacked on a `#[test]` function — the
        // test deliberately exercises the lint-triggering pattern (a reversed
        // range to verify a panic), so the clippy suppression is self-evident.
        // The `#[test]` is a *sibling* attribute, so the ancestor-walk
        // `is_in_test_context` misses it; `decorates_test_item` catches it.
        assert!(
            run("#[test]\n#[should_panic]\n#[allow(clippy::reversed_empty_ranges)]\nfn test_random_range_panic_int() {\n    let mut r = rng(102);\n    r.random_range(5..-2);\n}")
                .is_empty()
        );
        // Same pattern wrapped in the idiomatic `#[cfg(test)] mod test` — exempt
        // via the module-level cfg(test) too.
        assert!(
            run("#[cfg(test)]\nmod test {\n#[test]\n#[allow(clippy::bool_assert_comparison)]\nfn t() {\n    assert_eq!(b, false);\n}\n}")
                .is_empty()
        );
    }

    #[test]
    fn flags_clippy_outside_test_context() {
        // Load-bearing guard: the clippy exemption is test-scoped only; an
        // unjustified `#[allow(clippy::*)]` on an ordinary fn stays flagged.
        assert_eq!(run("#[allow(clippy::reversed_empty_ranges)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn ignores_deprecated_in_test_context() {
        // #4679: test suites call deprecated APIs inside `#[cfg(test)]` to verify
        // backward-compat behavior; the test context makes the reason self-evident.
        assert!(
            run("#[cfg(test)]\nmod test {\n#[test]\nfn north_bearing() {\n#[allow(deprecated)]\nlet bearing = p_1.geodesic_bearing(p_2);\n}\n}")
                .is_empty()
        );
        assert!(
            run("#[cfg(test)]\nmod test {\n#[allow(deprecated)]\nuse crate::RhumbDistance;\n}")
                .is_empty()
        );
    }

    #[test]
    fn flags_deprecated_outside_test_context() {
        // Load-bearing guard: the bare deprecated exemption is test-scoped only;
        // an ordinary `fn f()` is neither a deprecated trait method nor
        // `#[deprecated]`, so it stays flagged.
        assert_eq!(run("#[allow(deprecated)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_deprecated_in_deprecated_trait_method_impl() {
        // #5204: implementing a deprecated `std::error::Error` trait method on a
        // wrapper forces a delegating call to the inner type's deprecated method;
        // the deprecated context is the justification.
        assert!(
            run("impl StdError for BoxedError {\n    fn description(&self) -> &str {\n        #[allow(deprecated)]\n        self.0.description()\n    }\n}")
                .is_empty()
        );
        assert!(
            run("impl StdError for BoxedError {\n    fn cause(&self) -> Option<&dyn StdError> {\n        #[allow(deprecated)]\n        self.0.cause()\n    }\n}")
                .is_empty()
        );
    }

    #[test]
    fn allows_deprecated_inside_deprecated_fn() {
        // A `#[deprecated]` function that maintains deprecated code self-justifies
        // an inner `#[allow(deprecated)]`.
        assert!(
            run("#[deprecated]\nfn old_api() {\n    #[allow(deprecated)]\n    legacy_call();\n}")
                .is_empty()
        );
    }

    #[test]
    fn flags_deprecated_in_ordinary_method() {
        // Load-bearing guard: the deprecated-context exemption keys on the
        // function name / `#[deprecated]` attribute, not on being inside any fn.
        assert_eq!(
            run("impl Foo for Bar {\n    fn run(&self) {\n        #[allow(deprecated)]\n        legacy_call();\n    }\n}").len(),
            1
        );
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
    fn allows_dead_code_with_cfg_separated_by_other_attribute() {
        // #7111: `#[cfg(not(test))]` justifies the dead_code allow even when
        // `#[inline]` sits between the cfg and the allow in the attribute stack.
        assert!(
            run("#[cfg(not(test))]\n#[inline]\n#[allow(dead_code)]\npub fn f() {}").is_empty()
        );
    }

    #[test]
    fn allows_dead_code_with_cfg_after_allow_and_further_in_stack() {
        // The cfg gate may sit after the allow, and more than one attribute away
        // in either direction.
        assert!(
            run("#[allow(dead_code)]\n#[inline]\n#[cfg(feature = \"ffi\")]\npub fn f() {}")
                .is_empty()
        );
        assert!(
            run("#[cfg(not(test))]\n#[inline]\n#[cold]\n#[allow(dead_code)]\npub fn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn flags_dead_code_with_non_cfg_attribute_in_stack() {
        // Load-bearing guard: a non-cfg attribute in the stack is not a gate, so
        // it does not justify the allow — only a `#[cfg(...)]` does.
        assert_eq!(run("#[inline]\n#[allow(dead_code)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_dead_code_when_cfg_is_on_a_different_item() {
        // Load-bearing guard: a `#[cfg(...)]` decorating a *different* item does
        // not leak across the item boundary to justify a later allow.
        assert_eq!(
            run("#[cfg(test)]\nfn a() {}\n#[allow(dead_code)]\nfn b() {}").len(),
            1
        );
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
    fn allows_with_trailing_inline_comment_on_following_line() {
        // #6945: the justification is a trailing inline comment on the code line
        // immediately after the `#[allow]` (here a match arm), not a standalone
        // comment that starts with `//`.
        assert!(
            run("fn f() {\n    match last_id.checked_add(1) {\n        Some(id) => id..=u32::MAX,\n        #[allow(clippy::reversed_empty_ranges)]\n        None => 1..=0, // empty range iterator\n    };\n}")
                .is_empty()
        );
    }

    #[test]
    fn flags_following_line_without_comment() {
        // Load-bearing guard: the same match-arm shape with no trailing comment
        // on the following line stays flagged.
        assert_eq!(
            run("fn f() {\n    match last_id.checked_add(1) {\n        Some(id) => id..=u32::MAX,\n        #[allow(clippy::reversed_empty_ranges)]\n        None => 1..=0,\n    };\n}").len(),
            1
        );
    }

    #[test]
    fn flags_following_line_with_slashes_only_inside_string() {
        // Load-bearing guard: a `//` that appears only inside a string literal on
        // the following line is not a comment, so it does not justify the allow.
        assert_eq!(
            run("#[allow(clippy::foo)]\nfn f() -> usize { \"x//y\".len() }").len(),
            1
        );
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
    fn allows_self_justifying_non_upper_case_globals() {
        // #5455: a const whose name mirrors an external identifier (IANA tz
        // names) carries `#[allow(non_upper_case_globals)]` as a structural
        // opt-out; the lint name is the reason.
        assert!(
            run("#[allow(non_upper_case_globals)]\npub const Africa__Abidjan: Self = Self::Tz(chrono_tz::Africa::Abidjan);")
                .is_empty()
        );
    }

    #[test]
    fn allows_self_justifying_non_camel_case_types() {
        // #5551: ash's Vulkan FFI bindings carry `#[allow(non_camel_case_types)]`
        // on C-style type aliases (`VK_MAKE_API_VERSION`); the lint name is the
        // reason — the names come from the Vulkan spec and cannot be renamed.
        assert!(run("#[allow(non_camel_case_types)]\npub type VK_MAKE_API_VERSION = ();").is_empty());
    }

    #[test]
    fn allows_self_justifying_non_snake_case() {
        // Same naming-convention family: a binding mirroring a foreign C symbol.
        assert!(run("#[allow(non_snake_case)]\npub fn VkCreateInstance() {}").is_empty());
    }

    #[test]
    fn flags_concern_lint_on_foreign_named_item() {
        // Load-bearing guard: the exemption keys on the lint name, never on the
        // item; a genuine-concern lint like `dead_code` still requires a
        // justification even on a foreign-named type.
        assert_eq!(run("#[allow(dead_code)]\npub type VkFoo = ();").len(), 1);
    }

    #[test]
    fn allows_self_justifying_missing_docs() {
        // #5455: suppressing the missing-documentation lint is itself the
        // statement that the item is intentionally undocumented.
        assert!(
            run("#[allow(missing_docs)]\npub const Africa__Accra: Self = Self::Tz(chrono_tz::Africa::Accra);")
                .is_empty()
        );
    }

    #[test]
    fn allows_self_justifying_nonstandard_style() {
        assert!(run("#[allow(nonstandard_style)]\npub const Foo__Bar: u8 = 0;").is_empty());
    }

    #[test]
    fn allows_self_justifying_combined_list() {
        // #5455: the rrule timezone constants combine both lints in one list;
        // the loop must exempt only when *every* lint is self-justifying.
        assert!(
            run("#[allow(non_upper_case_globals, missing_docs)]\npub const Africa__Abidjan: u8 = 0;")
                .is_empty()
        );
    }

    #[test]
    fn flags_mixed_self_justifying_and_concern_lint() {
        // Load-bearing guard: a list mixing a self-justifying lint with a
        // genuine-concern lint (`dead_code`) still requires a justification.
        assert_eq!(run("#[allow(missing_docs, dead_code)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_multiline_allow_without_inner_comment() {
        // Load-bearing guard: a multiline allow with no `//` in its span must
        // still be flagged — the inner scan must not blanket-exempt multiline.
        assert_eq!(run("#[allow(\n    foo,\n    bar,\n)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_subsequent_allow_in_commented_cluster() {
        // #6196: a single `//` comment above the first of a consecutive `#[allow]`
        // cluster documents the whole stack; the second allow is justified too.
        assert!(
            run("// allow(unknown_lints) can be removed at rust-version 1.86.0, see:\n// https://example.com\n#[allow(unknown_lints)]\n#[allow(clippy::double_ended_iterator_last)]\nfn choose_stable() {}")
                .is_empty()
        );
    }

    #[test]
    fn allows_cluster_with_multiline_first_member() {
        // The adjacency walk keys on each sibling's *end* row, so a multiline
        // first member (closing `)]` on its own line) keeps the cluster intact.
        assert!(
            run("// shared justification\n#[allow(\n    foo,\n)]\n#[allow(bar)]\nfn f() {}").is_empty()
        );
    }

    #[test]
    fn flags_allow_separated_from_comment_by_blank_line() {
        // Load-bearing guard: a blank line breaks the cluster, so the comment no
        // longer documents the second allow — it is flagged while the first
        // (commented) allow is not.
        assert_eq!(
            run("// justification\n#[allow(unknown_lints)]\n\n#[allow(dead_code)]\nfn f() {}").len(),
            1
        );
    }
}
