//! rust-prefer-strum backend.
//!
//! On each `source_file` node, scans direct children for:
//! 1. `enum_item` nodes whose variants are all unit variants — collect their
//!    names. Enums with any data-carrying (tuple/struct) variant are skipped:
//!    `strum`'s derives cannot represent a variant's payload.
//! 2. `impl_item` nodes whose `trait` text matches a `Display` path —
//!    record the target type name.
//! 3. `impl_item` nodes whose `trait` text matches a `FromStr` path AND
//!    whose `type Err` associated type is the unit type `()` —
//!    record the target type name.
//!
//! An enum name present in BOTH (2) and (3) is flagged on its `enum_item` node
//! only when its `FromStr` impl is a clean 1:1 round-trip with the variants:
//! the `from_str` `match` has exactly one value arm (a single string-literal
//! pattern mapping to `Ok(<Variant>)`) per enum variant, no two value arms
//! target the same variant, and every other arm is a single catch-all error
//! arm (`_ => Err(..)` / `v => Err(..)`). Any or-pattern, guarded arm, or value
//! arm whose body is not `Ok(<single path>)` makes the pair non-1:1 and is not
//! flagged. Only a clean round-trip is reproducible by `#[derive(strum::Display,
//! strum::EnumString)]`; aliasing (two strings → one variant), dropped variants
//! (a variant with no `FromStr` arm), or asymmetric mappings would be silently
//! broken by the derive.
//!
//! The two clean impls together are the redundancy `strum::Display` +
//! `strum::EnumString` is designed to remove.
//!
//! `strum::EnumString` hardcodes `type Err = strum::ParseError` and can only
//! report `VariantNotFound`. A `FromStr` whose `Err` is anything other than
//! `()` carries a richer contract (custom error type and/or messages) that the
//! derive cannot reproduce, so migrating it would be behavior-destroying — such
//! impls are not recorded as targets.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let mut enums: Vec<(tree_sitter::Node, String)> = Vec::new();
    let mut display_targets: FxHashSet<String> = FxHashSet::default();
    let mut from_str_impls: FxHashMap<String, tree_sitter::Node> = FxHashMap::default();

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "enum_item" => {
                if let Some(name_node) = child.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
                    && !enum_has_data_variant(child)
                {
                    enums.push((child, name.to_string()));
                }
            }
            "impl_item" => {
                let Some(trait_node) = child.child_by_field_name("trait") else {
                    continue;
                };
                let Some(type_node) = child.child_by_field_name("type") else {
                    continue;
                };
                let Ok(trait_text) = trait_node.utf8_text(source) else {
                    continue;
                };
                let Ok(type_text) = type_node.utf8_text(source) else {
                    continue;
                };
                let trait_trimmed = trait_text.trim();
                let type_trimmed = type_text.trim().to_string();
                if is_display_trait(trait_trimmed) {
                    display_targets.insert(type_trimmed);
                } else if is_from_str_trait(trait_trimmed) && from_str_has_unit_err(child, source) {
                    from_str_impls.insert(type_trimmed, child);
                }
            }
            _ => {}
        }
    }

    for (enum_node, name) in enums {
        let Some(&from_str_impl) = from_str_impls.get(&name) else {
            continue;
        };
        if display_targets.contains(&name)
            && from_str_is_clean_round_trip(from_str_impl, enum_node, source)
        {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &enum_node,
                super::META.id,
                format!(
                    "Enum `{name}` has manual `Display` + `FromStr` impls. \
                     Replace both with `#[derive(strum::Display, strum::EnumString)]` \
                     (use `#[strum(serialize = \"...\")]` per-variant if the string \
                     form differs from the variant name) so the round-trip stays in \
                     sync by construction."
                ),
                Severity::Warning,
            ));
        }
    }
}

fn is_display_trait(text: &str) -> bool {
    matches!(
        text,
        "Display" | "fmt::Display" | "std::fmt::Display" | "core::fmt::Display"
    )
}

fn is_from_str_trait(text: &str) -> bool {
    matches!(
        text,
        "FromStr" | "str::FromStr" | "std::str::FromStr" | "core::str::FromStr"
    )
}

/// True when any variant of `enum_node` carries data — i.e. has a
/// `field_declaration_list` (struct variant) or `ordered_field_declaration_list`
/// (tuple variant) child.
///
/// `strum::Display`/`EnumString` map only variant name <-> string; they have no
/// representation for a variant's payload. An enum whose manual impls
/// serialize/parse that payload (e.g. `terminal_42` <-> `Terminal(42)`) would be
/// silently broken by the derive, so such enums must not be flagged.
fn enum_has_data_variant(enum_node: tree_sitter::Node) -> bool {
    let Some(body) = enum_node.child_by_field_name("body") else {
        return false;
    };
    let mut variant_cursor = body.walk();
    for variant in body.named_children(&mut variant_cursor) {
        if variant.kind() != "enum_variant" {
            continue;
        }
        let mut field_cursor = variant.walk();
        for child in variant.named_children(&mut field_cursor) {
            if matches!(
                child.kind(),
                "field_declaration_list" | "ordered_field_declaration_list"
            ) {
                return true;
            }
        }
    }
    false
}

/// True when the `FromStr` `impl_item`'s body declares `type Err = ();`.
///
/// `()` is the only error type whose replacement by `strum::EnumString`
/// (which forces `type Err = strum::ParseError`) loses no information. A named
/// type (`anyhow::Error`, `InvalidChecksum`, …) — or a missing `Err` — means
/// the migration would change the public `FromStr::Err` and/or drop custom
/// messages, so the enum must not be flagged.
fn from_str_has_unit_err(impl_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = impl_node.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    for item in body.named_children(&mut cursor) {
        if item.kind() == "type_item"
            && let Some(name) = item.child_by_field_name("name")
            && name.utf8_text(source) == Ok("Err")
        {
            return item
                .child_by_field_name("type")
                .is_some_and(|ty| ty.kind() == "unit_type");
        }
    }
    false
}

/// True when `impl_node`'s `from_str` is a clean 1:1 round-trip with the
/// enum's variants: exactly one value arm (a single string-literal pattern
/// mapping to `Ok(<Variant>)`) per variant, no two value arms targeting the
/// same variant, and every other arm a single catch-all error arm
/// (`_ => Err(..)` / `v => Err(..)`).
///
/// Any irregular arm — an or-pattern (`"a" | "b" =>`), a guarded arm
/// (`s if .. =>`), a value arm whose body is not `Ok(<single path>)`, or a
/// binding arm whose body is not `Err(..)` — means the pair is not a
/// strum-expressible round-trip, so the function returns `false`. It also
/// returns `false` for any node shape it cannot confidently classify:
/// under-flagging is the safe direction for a false-positive fix.
fn from_str_is_clean_round_trip(
    impl_node: tree_sitter::Node,
    enum_node: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Some(variant_count) = enum_variant_count(enum_node) else {
        return false;
    };
    let Some(match_block) = from_str_match_block(impl_node) else {
        return false;
    };

    let mut value_targets: FxHashSet<String> = FxHashSet::default();
    let mut value_arm_count = 0usize;
    let mut cursor = match_block.walk();
    for arm in match_block.named_children(&mut cursor) {
        if arm.kind() != "match_arm" {
            continue;
        }
        match classify_arm(arm, source) {
            ArmKind::Value(variant) => {
                value_arm_count += 1;
                value_targets.insert(variant);
            }
            ArmKind::Error => {}
            ArmKind::Irregular => return false,
        }
    }

    value_arm_count == variant_count && value_targets.len() == value_arm_count
}

enum ArmKind {
    /// A clean value arm: a single string-literal pattern → `Ok(<Variant>)`.
    /// Carries the target variant's identifier.
    Value(String),
    /// A single catch-all error arm: `_ => Err(..)` / `v => Err(..)`.
    Error,
    /// Anything else (or-pattern, guard, non-`Ok`/`Err` body, multiple
    /// literals): not a clean round-trip.
    Irregular,
}

/// Number of `enum_variant` children in `enum_node`'s body, or `None` when the
/// body is absent.
fn enum_variant_count(enum_node: tree_sitter::Node) -> Option<usize> {
    let body = enum_node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let count = body
        .named_children(&mut cursor)
        .filter(|v| v.kind() == "enum_variant")
        .count();
    Some(count)
}

/// The `match_block` of the FIRST `match_expression` inside the impl's
/// `from_str` function body.
fn from_str_match_block(impl_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let body = impl_node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let from_str_fn = body
        .named_children(&mut cursor)
        .find(|item| item.kind() == "function_item")?;
    let fn_body = from_str_fn.child_by_field_name("body")?;
    let match_expr = first_descendant(fn_body, "match_expression")?;
    match_expr.child_by_field_name("body")
}

/// First descendant of `node` (pre-order) whose kind matches `kind`.
fn first_descendant<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = first_descendant(child, kind) {
            return Some(found);
        }
    }
    None
}

/// Classify one `match_arm` as a clean value arm, a catch-all error arm, or
/// irregular. Defaults to `Irregular` for any shape not confidently a clean
/// value or error arm.
fn classify_arm(arm: tree_sitter::Node, source: &[u8]) -> ArmKind {
    let Some(pattern) = arm.child_by_field_name("pattern") else {
        return ArmKind::Irregular;
    };
    let Some(value) = arm.child_by_field_name("value") else {
        return ArmKind::Irregular;
    };
    // A guard (`s if ..`) adds a `condition` field to the `match_pattern`.
    if pattern.child_by_field_name("condition").is_some() {
        return ArmKind::Irregular;
    }

    let mut cursor = pattern.walk();
    let core: Vec<tree_sitter::Node> = pattern.named_children(&mut cursor).collect();

    match core.as_slice() {
        // `"literal" => ...`: candidate value arm.
        [single] if single.kind() == "string_literal" => {
            match ok_call_target(value, source) {
                Some(variant) => ArmKind::Value(variant),
                None => ArmKind::Irregular,
            }
        }
        // `v => Err(..)`: a bare binding catch-all error arm.
        [single] if single.kind() == "identifier" => {
            if is_err_call(value, source) {
                ArmKind::Error
            } else {
                ArmKind::Irregular
            }
        }
        // `_ => Err(..)`: a wildcard catch-all error arm (no named children).
        [] if is_wildcard(pattern) => {
            if is_err_call(value, source) {
                ArmKind::Error
            } else {
                ArmKind::Irregular
            }
        }
        // or-pattern, multiple literals, ranges, etc.
        _ => ArmKind::Irregular,
    }
}

/// True when `match_pattern` is a bare wildcard `_` (its only child is the
/// unnamed `_` token).
fn is_wildcard(pattern: tree_sitter::Node) -> bool {
    let mut cursor = pattern.walk();
    pattern.children(&mut cursor).any(|c| c.kind() == "_")
}

/// When `value` is `Ok(<single path>)`, return the LAST path-segment identifier
/// (the target variant). Otherwise `None`.
fn ok_call_target(value: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let arg = single_call_arg(value, "Ok", source)?;
    last_path_segment(arg, source)
}

/// True when `value` is `Err(..)` with a single argument.
fn is_err_call(value: tree_sitter::Node, source: &[u8]) -> bool {
    single_call_arg(value, "Err", source).is_some()
}

/// When `value` is a `call_expression` whose function text equals `func` and
/// whose `arguments` hold exactly one argument node, return that argument.
fn single_call_arg<'a>(
    value: tree_sitter::Node<'a>,
    func: &str,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    if value.kind() != "call_expression" {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if function.utf8_text(source).ok()?.trim() != func {
        return None;
    }
    let arguments = value.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let args: Vec<tree_sitter::Node> = arguments.named_children(&mut cursor).collect();
    match args.as_slice() {
        [single] => Some(*single),
        _ => None,
    }
}

/// The last path-segment identifier of a variant path. Accepts
/// `identifier` (`Variant`), `scoped_identifier` (`Enum::Variant`,
/// `crate::Enum::Variant`), returning the final segment. `None` for any other
/// node shape.
fn last_path_segment(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok().map(str::to_string),
        "scoped_identifier" => {
            let name = node.child_by_field_name("name")?;
            name.utf8_text(source).ok().map(str::to_string)
        }
        _ => None,
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
    fn flags_enum_with_both_display_and_from_str() {
        let src = r#"
enum Color { Red, Green, Blue }

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Color::Red => f.write_str("red"),
            Color::Green => f.write_str("green"),
            Color::Blue => f.write_str("blue"),
        }
    }
}

impl std::str::FromStr for Color {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "red" => Ok(Color::Red),
            "green" => Ok(Color::Green),
            "blue" => Ok(Color::Blue),
            _ => Err(()),
        }
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_enum_with_tuple_variant_payload() {
        // `PaneId` (zellij): the manual impls serialize/parse the variant
        // payload (`terminal_42` <-> `Terminal(42)`). `strum::Display`/
        // `EnumString` map only variant name <-> string and would silently
        // drop the payload, so a tuple-variant enum must never be flagged.
        let src = r#"
enum PaneId { Terminal(u32), Plugin(u32) }

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PaneId::Terminal(id) => write!(f, "terminal_{}", id),
            PaneId::Plugin(id) => write!(f, "plugin_{}", id),
        }
    }
}

impl std::str::FromStr for PaneId {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(n) = s.strip_prefix("terminal_") {
            n.parse().map(PaneId::Terminal).map_err(|_| ())
        } else if let Some(n) = s.strip_prefix("plugin_") {
            n.parse().map(PaneId::Plugin).map_err(|_| ())
        } else {
            Err(())
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_struct_variant_payload() {
        let src = r#"
enum E { A { x: u32 }, B }

impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            E::A { x } => write!(f, "a_{}", x),
            E::B => f.write_str("b"),
        }
    }
}

impl std::str::FromStr for E {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "b" => Ok(E::B),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_mixed_unit_and_data_variants() {
        let src = r#"
enum BareKey { Esc, F(u8), Char(char) }

impl std::fmt::Display for BareKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BareKey::Esc => f.write_str("esc"),
            BareKey::F(n) => write!(f, "f{}", n),
            BareKey::Char(c) => write!(f, "{}", c),
        }
    }
}

impl std::str::FromStr for BareKey {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "esc" => Ok(BareKey::Esc),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_from_str_custom_err_type() {
        // `Edition` (rust-lang/cargo): a `type Err = anyhow::Error` carrying
        // contextual `bail!` messages and a guarded `s if …` arm that
        // `strum::EnumString` (forced `type Err = strum::ParseError`) cannot
        // reproduce — migrating is behavior-destroying, so do not flag.
        let src = r#"
enum Edition { Edition2015, Edition2018 }

impl std::fmt::Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Edition::Edition2015 => f.write_str("2015"),
            Edition::Edition2018 => f.write_str("2018"),
        }
    }
}

impl std::str::FromStr for Edition {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2015" => Ok(Edition::Edition2015),
            "2018" => Ok(Edition::Edition2018),
            s => anyhow::bail!("supported edition values are `2015`, `2018`, but `{}` is unknown", s),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_from_str_named_err_type() {
        // `ChecksumAlgo` (rust-lang/cargo): `type Err = InvalidChecksum` is a
        // public-API contract `strum::EnumString` cannot preserve.
        let src = r#"
enum ChecksumAlgo { Sha256, Blake3 }

impl std::fmt::Display for ChecksumAlgo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChecksumAlgo::Sha256 => f.write_str("sha256"),
            ChecksumAlgo::Blake3 => f.write_str("blake3"),
        }
    }
}

impl std::str::FromStr for ChecksumAlgo {
    type Err = InvalidChecksum;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha256" => Ok(Self::Sha256),
            "blake3" => Ok(Self::Blake3),
            _ => Err(InvalidChecksum::InvalidChecksumAlgo),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_only_display() {
        let src = r#"
enum Color { Red, Green, Blue }

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Color::Red => f.write_str("red"),
            Color::Green => f.write_str("green"),
            Color::Blue => f.write_str("blue"),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_only_from_str() {
        let src = r#"
enum Color { Red, Green, Blue }

impl std::str::FromStr for Color {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "red" => Ok(Color::Red),
            "green" => Ok(Color::Green),
            "blue" => Ok(Color::Blue),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_strum_derive() {
        let src = r#"
#[derive(strum::Display, strum::EnumString)]
enum Color { Red, Green, Blue }
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_struct_with_display_and_from_str() {
        let src = r#"
struct Color(u32);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Color {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Color).map_err(|_| ())
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_enum_with_aliasing_named_err_type() {
        // `Severity` (biomejs/biome): `type Err = String` is already exempt via
        // the named-error gate. This guards that the biome repro — which is also
        // a non-round-trip (aliasing `"hint"`/`"info"` -> Information, no `Fatal`
        // parse arm) — stays empty even though Display + FromStr both exist.
        let src = r#"
enum Severity { Hint, Information, Warning, Error, Fatal }

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Severity::Hint => write!(f, "hint"),
            Severity::Information => write!(f, "info"),
            Severity::Warning => write!(f, "warn"),
            Severity::Error => write!(f, "error"),
            Severity::Fatal => write!(f, "fatal"),
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hint" => Ok(Self::Information),
            "info" => Ok(Self::Information),
            "warn" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            v => Err(format!("unexpected value ({v})")),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unit_err_aliasing_two_strings_one_variant() {
        // Two value arms target the same variant `A` (aliasing). strum's
        // `EnumString` maps each serialize string to its own variant and cannot
        // fold two inputs into one, so the round-trip is not strum-expressible.
        let src = r#"
enum E { A, B }

impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            E::A => f.write_str("a"),
            E::B => f.write_str("b"),
        }
    }
}

impl std::str::FromStr for E {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "a" => Ok(E::A),
            "x" => Ok(E::A),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unit_err_dropped_variant() {
        // `C` has no `FromStr` arm: 2 value arms != 3 variants. strum's
        // `EnumString` would generate a parse arm for `C`, changing behavior.
        let src = r#"
enum E { A, B, C }

impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            E::A => f.write_str("a"),
            E::B => f.write_str("b"),
            E::C => f.write_str("c"),
        }
    }
}

impl std::str::FromStr for E {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "a" => Ok(E::A),
            "b" => Ok(E::B),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unit_err_or_pattern_alias() {
        // An or-pattern (`"a" | "alpha"`) maps two strings to one variant; it is
        // not a clean 1:1 round-trip strum can express.
        let src = r#"
enum E { A, B }

impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            E::A => f.write_str("a"),
            E::B => f.write_str("b"),
        }
    }
}

impl std::str::FromStr for E {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "a" | "alpha" => Ok(E::A),
            "b" => Ok(E::B),
            _ => Err(()),
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
