//! rust-explicit-enum-match-arms backend.
//!
//! Walks every `match_expression`, looks at its arms, and flags a lone
//! `_` arm when at least one other arm has a pattern that "looks like"
//! an enum variant. See the module-level docblock in `mod.rs` for the
//! heuristic rationale.
//!
//! Pattern classification is purely syntactic:
//!
//! - "wildcard": node kind `wildcard_pattern`, or a pattern whose full
//!   text is exactly `_`.
//! - "enum-like": node kind is one of `scoped_identifier`,
//!   `tuple_struct_pattern`, `struct_pattern`, or the pattern text
//!   contains `::`, or it is a bare PascalCase identifier (uppercase
//!   lead with at least one lowercase letter). Literal patterns
//!   (`"AltLeft"`, `'r'`, `1`, `-2`, `true`), range patterns
//!   (`'a'..='z'`, `0..=9`) and SCREAMING_SNAKE_CASE constants
//!   (`EOF_CHAR`) apply only to scalar/string types and are never
//!   enum-like.
//!   Or-patterns (`Foo::A | Foo::B`) are unwrapped and any disjunct
//!   that qualifies makes the whole arm enum-like.
//!
//! Matches whose enum-like arms all reference a known stdlib closed or
//! non_exhaustive enum — `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
//! or `std::io::ErrorKind` — are exempt: the wildcard there is idiomatic
//! or compiler-mandated, and all arms of a `match` share one type.
//!
//! Matches with a non-wildcard arm carrying a match guard (`pat if cond`)
//! are exempt as a whole: a guarded arm never counts toward exhaustiveness
//! (the guard may be false at runtime), so the `_` arm is compiler-mandated
//! and listing every variant explicitly does not remove it.
//!
//! A wildcard arm whose body is a single diverging or error expression —
//! a `unreachable!`/`panic!`/`unimplemented!`/`todo!`/`bail!` macro
//! invocation, or `return Err(...)` (optionally wrapped in a
//! single-statement block) — is an explicit guard for the
//! impossible/error case, not a catch-all standing in for unenumerated
//! variants, so it is not flagged.
//!
//! A `_ => None` arm paired with at least one `Variant(v) => Some(v)` arm
//! is the variant-accessor idiom ("extract this variant, else nothing").
//! A later variant should still yield `None` here, so exhaustive listing
//! adds noise without safety, and the wildcard is not flagged.
//!
//! We do not descend into nested `match`es here — the walker visits
//! every `match_expression` independently, so each match is classified
//! on its own arms.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{arm_body_is_diverging, is_in_test_context};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["match_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let Some(match_block) = node.child_by_field_name("body") else {
            return;
        };

        // Walk the match_arm children, collecting wildcard arms and the
        // patterns of arms that look enum-like.
        let mut wildcard_arms: Vec<tree_sitter::Node> = Vec::new();
        let mut enum_like_arms: Vec<tree_sitter::Node> = Vec::new();
        // Tracks the `match self { Variant(v) => Some(v), _ => None }`
        // accessor idiom: at least one enum-like arm wraps its value in
        // `Some(...)`.
        let mut has_some_extracting_arm = false;
        let mut cursor = match_block.walk();
        for child in match_block.named_children(&mut cursor) {
            if child.kind() != "match_arm" {
                continue;
            }
            let Some(pattern) = child.child_by_field_name("pattern") else {
                continue;
            };
            // A match guard (`pat if cond => …`) on a non-wildcard arm
            // never counts toward exhaustiveness — the guard may be false
            // at runtime — so the compiler mandates a `_` arm regardless of
            // how many variants are listed. Listing every variant
            // explicitly does not remove that `_`, so flagging it is a
            // false positive: skip the whole match, like the other
            // whole-match exemptions.
            if !pattern_is_wildcard(pattern, source_bytes)
                && pattern_has_guard(pattern)
            {
                return;
            }
            if pattern_is_wildcard(pattern, source_bytes) {
                wildcard_arms.push(child);
            } else if pattern_is_enum_like(pattern, source_bytes) {
                enum_like_arms.push(pattern);
                if arm_body_is_some_call(child, source_bytes) {
                    has_some_extracting_arm = true;
                }
            }
        }

        if enum_like_arms.is_empty() {
            return;
        }
        // All arms of a `match` necessarily cover the same type, so when
        // every enum-like arm references a known stdlib closed or
        // non_exhaustive enum, the scrutinee is that stdlib type and the
        // wildcard is idiomatic (Result/Option) or compiler-mandated
        // (ErrorKind) — never a silent catch-all for a project enum.
        if enum_like_arms
            .iter()
            .all(|p| references_stdlib_closed_enum(*p, source_bytes))
        {
            return;
        }
        // Emit on each wildcard arm found (usually just one). A wildcard
        // arm whose body only diverges or returns an error
        // (`unreachable!()`, `panic!()`, `bail!(...)`, `return Err(...)`,
        // …) is a deliberate guard for the impossible/error case, not a
        // lazy catch-all to be replaced with enumerated variants — skip it.
        for arm in wildcard_arms {
            if arm_body_is_diverging(arm, source_bytes) {
                continue;
            }
            // A wildcard arm carrying its own `#[cfg(...)]` / `#[cfg_attr(...)]`
            // attribute is compiler-mandated and config-conditional: the
            // variant it covers only exists under that cfg, so it cannot be
            // listed explicitly (absent as source when the cfg is off) nor
            // removed (the match stops being exhaustive when the cfg is on).
            // Such an arm is not a lazy catch-all — skip it.
            if arm_has_cfg_attribute(arm, source_bytes) {
                continue;
            }
            // Variant-accessor idiom (issue #1252): a `_ => None` arm paired
            // with a `Variant(v) => Some(v)` arm is "extract this variant,
            // else nothing". A new variant should still yield `None` here, so
            // exhaustive listing adds noise without safety.
            if has_some_extracting_arm && arm_body_is_none(arm, source_bytes) {
                continue;
            }
            let pos = arm.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-explicit-enum-match-arms".into(),
                message: "Wildcard `_` arm in a `match` that appears to cover an enum. \
                          List each variant explicitly so adding a new variant produces \
                          a compile error at this `match`, forcing a decision instead of \
                          silently falling through."
                    .into(),
                severity: Severity::Warning,
                span: Some((arm.start_byte(), arm.end_byte() - arm.start_byte())),
            });
        }
    }
}

/// True if `pattern` is a bare wildcard `_`.
fn pattern_is_wildcard(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    if pattern.kind() == "wildcard_pattern" {
        return true;
    }
    // Fallback: some grammar versions may surface `_` as an identifier
    // or similar — trust the textual form only when it's exactly `_`.
    matches!(pattern.utf8_text(source), Ok("_"))
}

/// True if the arm's pattern carries a match guard (`pat if cond`).
/// tree-sitter-rust wraps the arm pattern in a `match_pattern` node whose
/// optional `condition` field is present exactly when a guard is written.
fn pattern_has_guard(pattern: tree_sitter::Node) -> bool {
    pattern.kind() == "match_pattern" && pattern.child_by_field_name("condition").is_some()
}

/// True if `pattern` looks like it matches an enum variant. See module
/// docblock for the heuristic.
fn pattern_is_enum_like(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // tree-sitter-rust wraps match arm patterns in a `match_pattern` node
    // (to accommodate guard clauses like `pat if cond`). Unwrap to the
    // inner pattern before classifying.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return pattern_is_enum_like(inner, source);
        }
        return false;
    }
    // Tuple patterns are product types: wildcard is always idiomatic
    // (covering N×M combinations of sub-arms is impractical).
    if pattern.kind() == "tuple_pattern" {
        return false;
    }
    // Range patterns (`'a'..='z'`, `0..=9`, `b'A'..=b'Z'`) only apply to
    // scalar types — `char`, integers, bytes — never enums. The `_` arm
    // on such a match is compiler-mandated, so a range is never enum-like.
    if pattern.kind() == "range_pattern" {
        return false;
    }
    // Or-pattern: recurse into each disjunct.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        for child in pattern.named_children(&mut cursor) {
            if pattern_is_enum_like(child, source) {
                return true;
            }
        }
        return false;
    }

    match pattern.kind() {
        "scoped_identifier" | "tuple_struct_pattern" | "struct_pattern" => return true,
        // Literal patterns match scalar/string values, never enum variants.
        // A `match key: &str { "AltLeft" => …, _ => … }` has an infinite
        // domain, so its `_` arm is compiler-mandated. Bail out before the
        // textual PascalCase fallback, which would otherwise skip the
        // opening quote of `"AltLeft"` and misread the literal as a variant.
        "string_literal" | "raw_string_literal" | "char_literal" | "integer_literal"
        | "float_literal" | "boolean_literal" | "negative_literal" => return false,
        _ => {}
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    if text.is_empty() || text == "_" {
        return false;
    }
    if text.contains("::") {
        return true;
    }
    // Bare uppercase identifiers are ambiguous: PascalCase ones look like
    // unqualified variants (`Some`, `None`, `North`), while
    // SCREAMING_SNAKE_CASE ones are named constants (`EOF_CHAR`, `NUL`)
    // matched in scalar lexers where the `_` arm is mandatory. Require a
    // lowercase letter so a const pattern is not treated as enum-like.
    let first_ident_char = text
        .chars()
        .find(|c| c.is_ascii_alphanumeric() || *c == '_');
    matches!(first_ident_char, Some(c) if c.is_ascii_uppercase())
        && text.chars().any(|c| c.is_ascii_lowercase())
}

/// True if `pattern` references a variant of a known stdlib closed or
/// non_exhaustive enum: `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
/// or `std::io::ErrorKind`. Matching is purely syntactic: the final path
/// segment of the variant head must be exactly one of the Result/Option
/// constructors, or the head must contain `ErrorKind::`.
fn references_stdlib_closed_enum(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // Unwrap the `match_pattern` wrapper, mirroring `pattern_is_enum_like`.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return references_stdlib_closed_enum(inner, source);
        }
        return false;
    }
    // Or-pattern: every disjunct must reference a stdlib enum.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        return pattern
            .named_children(&mut cursor)
            .all(|child| references_stdlib_closed_enum(child, source));
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    // Strip tuple-struct fields: `Err(e)` → `Err`, `Some(v)` → `Some`.
    let head = text.split('(').next().unwrap_or(text).trim();
    // Final path segment: `Result::Ok` → `Ok`, `Option::Some` → `Some`.
    let last_seg = head.rsplit("::").next().unwrap_or(head).trim();
    if matches!(last_seg, "Ok" | "Err" | "Some" | "None") {
        return true;
    }
    // `std::io::ErrorKind` is #[non_exhaustive]: a `_` arm is mandatory.
    head.contains("ErrorKind::")
}

/// True if the `match_arm`'s body is a `Some(...)` constructor call — the
/// "present" half of a variant-accessor (`Variant(v) => Some(v)`).
fn arm_body_is_some_call(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = value.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = callee.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == "Some"
}

/// True if the `match_arm`'s body is the bare `None` literal (optionally
/// path-qualified as `Option::None`) — the "absent" half of a
/// variant-accessor (`_ => None`).
fn arm_body_is_none(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    if !matches!(value.kind(), "identifier" | "scoped_identifier") {
        return false;
    }
    let Ok(text) = value.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == "None"
}

/// True if the `match_arm` node carries a leading `#[cfg(...)]` /
/// `#[cfg_attr(...)]` attribute. tree-sitter-rust attaches an arm's outer
/// attribute as an `attribute_item` *child* of the `match_arm` (verified
/// against the 0.23 grammar), shaped `attribute_item` → `attribute` whose
/// first named child `identifier` is the path (`cfg` / `cfg_attr`). The path
/// is matched exactly — not as a substring — so an unrelated attribute like
/// `#[allow(my_cfg_thing)]` does not qualify.
fn arm_has_cfg_attribute(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = arm.walk();
    arm.children(&mut cursor)
        .filter(|c| c.kind() == "attribute_item")
        .any(|attr_item| attribute_item_is_cfg(attr_item, source))
}

/// True if an `attribute_item`'s inner `attribute` has a leading path
/// identifier of exactly `cfg` or `cfg_attr`.
fn attribute_item_is_cfg(attr_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = attr_item.walk();
    attr_item
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "attribute")
        .filter_map(|attribute| attribute.named_child(0))
        .filter(|path| path.kind() == "identifier")
        .any(|path| matches!(path.utf8_text(source), Ok("cfg") | Ok("cfg_attr")))
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
    fn flags_wildcard_with_enum_variants() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, _ => 3 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_with_option_variants() {
        let src = "fn f(x: Option<i32>) -> i32 { match x { Some(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_result_variants() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Err(e) => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_errorkind() {
        let src = "fn f(e: std::io::Error) -> i32 { \
                   match e.kind() { ErrorKind::PermissionDenied => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_qualified_result() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Result::Ok(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_project_variant_resembling_ok() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::OkResponse => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_path_variants() {
        let src = "fn f(x: Direction) -> i32 { match x { Direction::North => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_all_variants_explicit() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, Foo::C => 3 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_integer_match() {
        let src = "fn f(x: i32) -> i32 { match x { 1 => 10, 2 => 20, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_wildcard_arm() {
        let src = "fn f(x: i32) -> i32 { match x { _ => 42 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_or_patterns() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A | Foo::B => 1, Foo::C => 2 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = "#[test]\nfn t() { let x = Foo::A; let _ = match x { Foo::A => 1, _ => 2 }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_options() {
        let src = "fn f(x: (Option<i32>, Option<i32>)) -> i32 { \
                   match x { (Some(a), Some(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_results() {
        let src = "fn f(x: (Result<i32, E>, Result<i32, E>)) -> i32 { \
                   match x { (Ok(a), Ok(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_char_literal_arms_with_enum_bodies() {
        // Issue #1409: scrutinee is a `char`; arm patterns are char
        // literals (not enum variants), and the `_` arm is compiler-
        // mandated because `char` cannot be enumerated. Enum names in the
        // arm bodies must not make this look enum-like.
        let src = "fn f(c: char) -> i32 { match c { \
                   'r' => CFormatType::Repr, \
                   's' => CFormatType::Str, \
                   _ => return Err(0), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_byte_literal_arms() {
        // Issue #1409: scrutinee is a `u8` byte; literal byte patterns
        // cannot be enumerated, so the `_` arm is required.
        let src = "fn f(b: u8) -> i32 { match b { \
                   b'a' => 1, b'b' => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_integer_arms_with_enum_bodies() {
        // Issue #1409: scrutinee is an `i32`; integer literal patterns
        // with enum-valued bodies must not be flagged.
        let src = "fn f(n: i32) -> Token { match n { \
                   1 => Token::One, 2 => Token::Two, _ => Token::Other } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_char_range_patterns() {
        // Issue #1409: range patterns apply only to scalar types, so the
        // uppercase bound `'A'` must not be read as an enum variant.
        let src = "fn classify(c: char) -> i32 { match c { \
                   'A'..='Z' => 1, 'a'..='z' => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_named_char_const_patterns() {
        // Issue #1409: SCREAMING_SNAKE_CASE patterns are named constants
        // (lexer sentinels like `EOF_CHAR`/`NUL`), not enum variants.
        let src = "fn lex(c: char) -> i32 { match c { \
                   EOF_CHAR => 0, NUL => 1, '0'..='9' => 2, _ => 3 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_str_literal_arms_with_enum_bodies() {
        // Issue #3973: scrutinee is a `&str` (egui `Key::from_name`); the
        // arm patterns are string literals whose content starts uppercase
        // (`"AltLeft"`, `"Exclamationmark"`), not enum variants. A `&str`
        // has an infinite domain, so the `_ => return None` arm is
        // compiler-mandated and must not be flagged.
        let src = "fn from_name(key: &str) -> Option<Self> { Some(match key { \
                   \"AltLeft\" => Self::AltLeft, \
                   \"!\" | \"Exclamationmark\" => Self::Exclamationmark, \
                   \"IntlBackslash\" => Self::IntlBackslash, \
                   _ => return None, }) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_raw_str_literal_arms() {
        // Issue #3973: raw string literals are also literal patterns, never
        // enum variants.
        let src = "fn f(s: &str) -> i32 { match s { \
                   r#\"Alpha\"# => 1, r#\"Beta\"# => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_negative_integer_arms() {
        // Issue #3973: negative integer literals are scalar patterns over
        // an unbounded domain; the `_` arm is required.
        let src = "fn f(n: i32) -> i32 { match n { -1 => 1, 0 => 2, 1 => 3, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_unreachable_body() {
        // Issue #1427: `_ => unreachable!()` documents that only specific
        // variants are reachable here — a deliberate guard, not a lazy
        // catch-all.
        let src = "fn f(msg: AnyMessage) -> Bytes { let b = match msg { \
                   AnyMessage::Bytes(b) => b, _ => unreachable!() }; b }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_bail_body() {
        // Issue #1427: protocol state machine where only certain variants
        // are valid; `_ => bail!(...)` errors on anything else.
        let src = "fn f(msg: ProposerAcceptorMessage) -> Result<(), E> { match msg { \
                   ProposerAcceptorMessage::Greeting(ref g) => handle(g), \
                   _ => bail!(\"unexpected message {msg:?} instead of greeting\"), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_bail_block_body() {
        // Issue #1427: same guard wrapped in a block, as in the issue.
        let src = "fn f(msg: Msg) -> Result<(), E> { match msg { \
                   Msg::Greeting(ref g) => handle(g), \
                   _ => { bail!(\"unexpected message {msg:?} instead of greeting\"); } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_return_err_body() {
        // Issue #1427: `_ => return Err(...)` is an explicit error path.
        let src = "fn f(x: Foo) -> Result<i32, E> { match x { \
                   Foo::A => Ok(1), _ => return Err(E::Unexpected), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_panic_body() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, _ => panic!(\"nope\") } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_with_ordinary_body() {
        // True positive: a lazy catch-all over an enum still fires even
        // though the diverging-arm exemption exists.
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_arm_doing_work_before_diverging() {
        // True positive: a block that runs other statements before
        // bailing is a real catch-all, not a bare guard.
        let src = "fn f(x: Foo) -> Result<i32, E> { match x { \
                   Foo::A => Ok(1), _ => { log(\"hit\"); bail!(\"unexpected\"); } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_variant_accessor_returning_none() {
        // Issue #1252: the idiomatic `match self { Variant(v) => Some(v),
        // _ => None }` accessor extracts one variant; the `_ => None` arm
        // is the intentional fallthrough and must not be flagged.
        let src = "fn import(self) -> Option<ImportId> { match self { \
                   ImportOrExternCrate::Import(it) => Some(it), _ => None } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_doing_real_work() {
        // Issue #1252 negative space (a): a `_` arm that calls a method is
        // a real catch-all, not a trivial accessor fallthrough.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A(v) => v, _ => self.compute() } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_arm_returning_nontrivial_value() {
        // Issue #1252 negative space (b): a `_` arm returning a non-trivial
        // constructed value over an enum still needs explicit variants.
        let src = "fn f(x: Foo) -> Bar { match x { \
                   Foo::A => Bar::One, _ => Bar::build(x, 7) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_with_guarded_enum_arm() {
        // Issue #3957: a non-wildcard arm carrying a match guard never
        // counts toward exhaustiveness, so the `_` arm is compiler-mandated
        // regardless of how many variants are listed.
        let src = "fn f(m: Foo) -> i32 { match m { \
                   Foo::Bar(x) if cond(x) => a(), _ => b() } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_prost_name_value_guarded_arm() {
        // Issue #3957: prost-derive `get_prost_path` shape — a guarded
        // `Meta::NameValue(..)` arm followed by `_ => continue`.
        let src = "fn g(attr: Meta) { match attr { \
                   Meta::NameValue(MetaNameValue { path, .. }) if path.is_ident(\"prost_path\") => take(), \
                   _ => continue, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_prost_lit_guarded_arms() {
        // Issue #3957: prost-derive `DefaultValue::from_lit` shape — guarded
        // `Lit::Int(..)` arms followed by `_ => ()`.
        let src = "fn h(lit: Lit, ty: Ty) { match lit { \
                   Lit::Int(ref lit) if ty == Ty::Float && lit.suffix().is_empty() => f(), \
                   Lit::Int(ref lit) if ty == Ty::Double && lit.suffix().is_empty() => d(), \
                   _ => (), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unguarded_enum_match_with_wildcard() {
        // Issue #3957 negative space: only a match guard exempts. An
        // unguarded enum match (variants + `_`) must still flag.
        let src = "fn f(d: Direction) -> i32 { match d { \
                   Direction::North => 1, Direction::South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_cfg_gated_wildcard_arm_swc_shape() {
        // Issue #3918: swc's `lit.rs` set_span — all real variants listed
        // explicitly plus a `_` arm gated by `#[cfg(all(swc_ast_unknown,
        // feature = "encoding-impl"))]`. The arm is compiler-mandated and
        // config-conditional, so it must not be flagged.
        let src = "fn set_span(self, span: Span) { match self { \
                   Lit::Str(s) => s.span = span, \
                   Lit::Bool(b) => b.span = span, \
                   Lit::Num(n) => n.span = span, \
                   #[cfg(all(swc_ast_unknown, feature = \"encoding-impl\"))] \
                   _ => swc_common::unknown!(), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_minimal_cfg_gated_wildcard_arm() {
        // Issue #3918: the minimal shape — a feature-gated `_` arm.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[cfg(feature = \"x\")] _ => 0, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cfg_attr_gated_wildcard_arm() {
        // Issue #3918: `#[cfg_attr(...)]` on the `_` arm is also exempt.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[cfg_attr(test, allow(unused))] _ => 0, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_with_non_cfg_attribute() {
        // Issue #3918 negative space: only `cfg`/`cfg_attr` exempts. A
        // wildcard arm carrying an unrelated attribute (here one whose token
        // tree even contains the substring `cfg`) is still a lazy catch-all.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[allow(my_cfg_thing)] _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_bare_pascal_case_variants() {
        // True positive: unqualified PascalCase variants (e.g. via
        // `use Direction::*`) still need explicit arms.
        let src = "use Direction::*; \
                   fn f(x: Direction) -> i32 { match x { North => 1, South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
