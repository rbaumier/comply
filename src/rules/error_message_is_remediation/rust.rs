//! error-message-is-remediation Rust backend.
//!
//! Flags vague error messages in `panic!("...")`, `anyhow!("...")`,
//! `bail!("...")`, and `Err("...")` / `Err(format!("..."))`.
//!
//! Test code is exempt: files under a test directory, and `panic!` inside
//! inline `#[test]` functions or `#[cfg(test)]` modules, are test-failure
//! signals rather than user-facing errors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, trait_base_name};
use tree_sitter::Node;

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass", "set", "add", "remove",
    "update", "create", "delete", "call", "return", "expect", "require", "missing", "failed",
    "cannot", "unable", "exceeded", "denied", "rejected", "not", "invalid", "unknown", "unexpected",
    "mismatched", "duplicate", "no", "none", "out", "exceeds", "expected", "wrong", "parse",
    "render", "compile", "load", "find",
];

/// True if `word` (already lowercased) is a listed verb in its base form or a
/// recognized inflection of one: third-person `-s` ("requires" → "require"),
/// gerund `-ing` ("rendering" → "render", "parsing" → "parse" with the dropped
/// silent `e` restored), past-tense `-ed`/`-d` ("passed" → "pass", "used" →
/// "use"), or `n't` contraction ("couldn't" → "could").
///
/// Only accepts a token whose stem, after stripping a single recognized suffix,
/// is itself a listed verb — an unknown `-s`/`-ing`/`-ed` word stays a non-verb.
fn token_is_verb(word: &str) -> bool {
    if VERBS.contains(&word) {
        return true;
    }
    if let Some(stem) = word.strip_suffix("n't") {
        if VERBS.contains(&stem) {
            return true;
        }
    }
    if let Some(stem) = word.strip_suffix("ing") {
        // Bare stem ("loading" → "load"), or a listed verb whose silent trailing
        // `e` was dropped before `-ing` ("parsing" → "parse", "rendering" → "render").
        if VERBS.contains(&stem) || VERBS.iter().any(|v| v.strip_suffix('e') == Some(stem)) {
            return true;
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        if VERBS.contains(&stem) {
            return true;
        }
    }
    if let Some(stem) = word.strip_suffix("ed") {
        // Past tense of a consonant-ending verb ("passed" → "pass",
        // "rendered" → "render").
        if VERBS.contains(&stem) {
            return true;
        }
    }
    if let Some(stem) = word.strip_suffix('d') {
        // Past tense of a verb whose silent trailing `e` is kept before `-d`
        // ("used" → "use", "required" → "require", "provided" → "provide").
        if VERBS.contains(&stem) {
            return true;
        }
    }
    false
}

fn has_verb(msg: &str) -> bool {
    msg.to_ascii_lowercase()
        .split_whitespace()
        .any(token_is_verb)
}

/// True if `node` sits in the body of a `fn default()` whose enclosing `impl`
/// is an `impl Default for T` block.
///
/// The `Default` supertrait is often a mandatory bound on a marker trait, so the
/// impl must exist even when the type is a zero-variant (uninhabited) marker that
/// can never be instantiated. The `default()` body is then an unreachable stub
/// that panics — a trait-satisfaction placeholder, not a user-facing error — so
/// its message need not read as remediation.
///
/// Only a trait impl qualifies: the nearest enclosing `impl_item` must carry a
/// `trait` field resolving to `Default` (bare `Default` or a path ending in it,
/// e.g. `core::default::Default`). An inherent `impl T { fn default() {} }` has no
/// `trait` field and does not qualify.
fn is_in_default_trait_stub(node: Node, source: &[u8]) -> bool {
    // Nearest enclosing function must be named `default`.
    let mut current = node.parent();
    let func = loop {
        let Some(ancestor) = current else { return false };
        if ancestor.kind() == "function_item" {
            break ancestor;
        }
        current = ancestor.parent();
    };
    let is_default_fn = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        == Some("default");
    if !is_default_fn {
        return false;
    }
    // That function's enclosing impl must be a trait impl for `Default`.
    let mut current = func.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor
                .child_by_field_name("trait")
                .and_then(|t| trait_base_name(t, source))
                .is_some_and(|name| name == "Default");
        }
        current = ancestor.parent();
    }
    false
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if ctx.file.path_segments.in_test_dir { return; }

    if mac_name != "panic" && mac_name != "bail" && mac_name != "anyhow" {
        return;
    }

    // Panics inside inline `#[test]` functions / `#[cfg(test)]` modules signal
    // a test failure, not a user-facing error — they need not read as
    // remediation.
    if is_in_test_context(node, source) { return; }

    // A `panic!` that is the body of `fn default()` inside an `impl Default for T`
    // block is an unreachable trait-satisfaction stub (the `Default` supertrait is
    // mandatory but the type may be uninhabited), not a user-facing error.
    if is_in_default_trait_stub(node, source) { return; }

    let Ok(full_text) = node.utf8_text(source) else { return };

    // Extract the first string literal argument. The closing delimiter is the
    // first `"` that is not escaped, so an embedded `\"` (common when a message
    // quotes a value, e.g. `op=\"fit_width\" requires …`) does not truncate the
    // text before its verb.
    let Some(open) = full_text.find('"') else { return };
    let after_open = &full_text[open + 1..];
    let mut close_rel = None;
    let mut escaped = false;
    for (idx, ch) in after_open.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => {
                close_rel = Some(idx);
                break;
            }
            _ => {}
        }
    }
    let Some(close_rel) = close_rel else { return };
    let msg = &after_open[..close_rel];

    // A literal that is nothing but a single bare format placeholder
    // (`{}`, `{:?}`, `{0}`, `{e}`, `{value:>8}`, …) carries no static text:
    // the runtime message is supplied entirely by the macro argument, so there
    // is no human-facing string to judge for length or verb content. The
    // interior must hold no further braces, so only a whole-string single
    // placeholder qualifies — `failed {}` keeps its surrounding prose and is
    // judged as before.
    if msg.starts_with('{')
        && msg.ends_with('}')
        && msg.len() >= 2
        && !msg[1..msg.len() - 1].contains(['{', '}'])
    {
        return;
    }

    let too_short = msg.len() < 15;
    let no_verb = !has_verb(msg);

    if too_short || no_verb {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-message-is-remediation".into(),
            message: "Error message is too vague — describe what went wrong and what to do.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    fn run_on_with_file_ctx(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.rs", crate::project::default_static_project_ctx(), file)
    }

    #[test]
    fn flags_short_panic() {
        assert_eq!(run_on(r#"fn f() { panic!("oops"); }"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_panic() {
        assert!(run_on(r#"fn f() { panic!("Connection pool is exhausted — try again or check configuration"); }"#).is_empty());
    }

    #[test]
    fn ignores_panic_in_test_file() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(run_on_with_file_ctx(r#"fn f() { panic!("oops"); }"#, &file).is_empty());
    }

    #[test]
    fn still_flags_panic_in_production() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: false, ..Default::default() },
            ..Default::default()
        };
        assert_eq!(run_on_with_file_ctx(r#"fn f() { panic!("oops"); }"#, &file).len(), 1);
    }

    #[test]
    fn ignores_panic_in_inline_test_fn() {
        let source = r#"#[test]
fn test_make_field_nullable() {
    panic!("Expected Struct type for list items");
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_panic_in_cfg_test_module() {
        let source = r#"#[cfg(test)]
mod tests {
    fn helper() {
        panic!("Expected Struct type for list items");
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_invalid_qualifier() {
        assert!(run_on(r#"fn f() { bail!("invalid map attribute: {attr:?}"); }"#).is_empty());
    }

    #[test]
    fn allows_unknown_qualifier() {
        assert!(
            run_on(r#"fn f() { bail!("unknown attribute(s) for message field"); }"#).is_empty()
        );
    }

    #[test]
    fn allows_no_qualifier() {
        assert!(run_on(r#"fn f() { bail!("no type attribute for oneof field"); }"#).is_empty());
    }

    #[test]
    fn allows_unexpected_qualifier() {
        assert!(run_on(r#"fn f() { bail!("unexpected end of input here"); }"#).is_empty());
    }

    #[test]
    fn no_qualifier_is_whole_word_not_substring() {
        // "note" and "without" contain "no" as a substring; "output" contains
        // "out". A message whose only candidate token is such a word must still
        // be flagged — matching is token-based, not substring-based.
        assert_eq!(
            run_on(r#"fn f() { bail!("note about wonky thingamajig output"); }"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_long_vague_no_verb_message() {
        assert_eq!(
            run_on(r#"fn f() { bail!("something wonky happened somewhere"); }"#).len(),
            1
        );
    }

    #[test]
    fn ignores_panic_in_default_trait_stub() {
        // BurntSushi/byteorder #6532: `BigEndian` is a zero-variant marker enum;
        // the `ByteOrder` trait requires `Default`, so this impl is mandatory but
        // structurally unreachable. The panic is a trait-satisfaction stub.
        let source = r#"impl Default for BigEndian {
    fn default() -> BigEndian {
        panic!("BigEndian default")
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_panic_in_default_trait_stub_path_qualified() {
        let source = r#"impl core::default::Default for LittleEndian {
    fn default() -> LittleEndian {
        panic!("LittleEndian default")
    }
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_panic_in_default_inherent_impl() {
        // Inherent `impl T { fn default() {} }` has no trait field — not a
        // `Default` trait stub, so a vague panic still flags.
        let source = r#"impl BigEndian {
    fn default() -> BigEndian {
        panic!("oops")
    }
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_panic_in_non_default_fn_of_impl_default() {
        // A vague panic in a non-`default` method sitting inside the same
        // `impl Default` block is not the trait stub and still flags; the
        // exempt `default()` stub in the block does not.
        let source = r#"impl Default for BigEndian {
    fn default() -> BigEndian {
        panic!("BigEndian default")
    }
    fn helper() -> BigEndian {
        panic!("oops")
    }
}"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn ignores_bare_placeholder_passthrough() {
        // getzola/zola #6561: the literal is a single bare placeholder; the whole
        // error text comes from the argument at runtime, so there is no static
        // string to judge.
        assert!(run_on(r#"fn f() { bail!("{}", fancy_e); }"#).is_empty());
        assert!(run_on(r#"fn f() { panic!("{}", err); }"#).is_empty());
    }

    #[test]
    fn ignores_bare_placeholder_with_format_spec() {
        assert!(run_on(r#"fn f() { bail!("{:?}", e); }"#).is_empty());
        assert!(run_on(r#"fn f() { bail!("{0}", e); }"#).is_empty());
        assert!(run_on(r#"fn f() { bail!("{value:>8}", value); }"#).is_empty());
    }

    #[test]
    fn still_flags_placeholder_embedded_in_prose() {
        // Text surrounds the placeholder, so the static prose is still judged and
        // a short/vague message keeps flagging.
        assert_eq!(run_on(r#"fn f() { bail!("failed: {}", e); }"#).len(), 1);
    }

    #[test]
    fn still_flags_multiple_placeholders() {
        // Only a whole-string single placeholder is exempt; interior braces (two
        // placeholders) keep the literal in scope of the length/verb checks.
        assert_eq!(run_on(r#"fn f() { bail!("{} {}", a, b); }"#).len(), 1);
    }

    #[test]
    fn still_flags_vague_static_message() {
        // No placeholder at all — a genuinely short static message still flags.
        assert_eq!(run_on(r#"fn f() { bail!("oops"); }"#).len(), 1);
    }

    #[test]
    fn allows_third_person_s_inflection() {
        // getzola/zola #6562: "requires" is the third-person form of "require".
        assert!(
            run_on(r#"fn f() { anyhow!("op=\"fit_width\" requires a width argument"); }"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_gerund_inflection() {
        // "parsing" is the gerund of "parse" (silent `e` dropped before `-ing`).
        assert!(run_on(r#"fn f() { anyhow!("Error parsing YAML datetime here"); }"#).is_empty());
    }

    #[test]
    fn allows_added_base_verbs_render_and_compile() {
        // templates.rs:117 — "render" is a listed base verb; the embedded `{}`
        // placeholder is surrounded by prose so the literal is still judged.
        assert!(
            run_on(r#"fn f() { bail!("Tried to render `{}` but the template wasn't found", name); }"#)
                .is_empty()
        );
        // sass.rs:43 — escaped `\"` around quoted paths must not truncate the
        // text before "compile".
        assert!(
            run_on(r#"fn f() { bail!("SASS path conflict: \"{}\" and \"{}\" both compile to \"{}\"", a, b, c); }"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_contracted_negation() {
        // "Couldn't" → "could"; "find" is also a listed base verb.
        assert!(
            run_on(r#"fn f() { panic!("Couldn't find section in the page index"); }"#).is_empty()
        );
    }

    #[test]
    fn still_flags_verbless_long_message_after_inflection_support() {
        // No base or inflected verb anywhere — stemming only accepts tokens whose
        // stem is a known verb, so a verbless message keeps flagging.
        assert_eq!(
            run_on(r#"fn f() { bail!("wonky thingamajig somewhere nearby"); }"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_unknown_inflected_word() {
        // "thingamajigs"/"gizmos" end in `-s` but their stems are not listed
        // verbs, so the message is still vague — stemming is not a blanket pass.
        assert_eq!(
            run_on(r#"fn f() { bail!("thingamajigs and gizmos everywhere abound"); }"#).len(),
            1
        );
    }

    #[test]
    fn allows_past_tense_ed_inflection() {
        // emilk/egui #7449: "passed" is the past tense of "pass" (a listed verb),
        // recognized by stripping `-ed` from a consonant-ending verb.
        assert!(
            run_on(r#"fn f() { panic!("Shape::Callback passed to Tessellator"); }"#).is_empty()
        );
    }

    #[test]
    fn allows_past_tense_d_inflection_silent_e_verbs() {
        // Silent-`e` verbs keep the `e` before `-d`: "used" → "use",
        // "required" → "require".
        assert!(run_on(r#"fn f() { anyhow!("scratch buffer used past cleanup"); }"#).is_empty());
        assert!(run_on(r#"fn f() { bail!("valid token required for this action"); }"#).is_empty());
    }

    #[test]
    fn still_flags_ed_word_with_non_verb_stem() {
        // "red" ends in `-ed`/`-d` but strips to "r"/"re", neither a listed verb;
        // with no other verb the message stays vague — stemming is not a blanket
        // pass for every `-ed` word.
        assert_eq!(run_on(r#"fn f() { bail!("red banner shown here"); }"#).len(), 1);
    }
}
