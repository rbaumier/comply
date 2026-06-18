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
//! Any enum name present in BOTH (2) and (3) is flagged on its
//! `enum_item` node. The two impls together are the redundancy
//! `strum::Display` + `strum::EnumString` is designed to remove.
//!
//! `strum::EnumString` hardcodes `type Err = strum::ParseError` and can only
//! report `VariantNotFound`. A `FromStr` whose `Err` is anything other than
//! `()` carries a richer contract (custom error type and/or messages) that the
//! derive cannot reproduce, so migrating it would be behavior-destroying — such
//! impls are not recorded as targets.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let mut enums: Vec<(tree_sitter::Node, String)> = Vec::new();
    let mut display_targets: HashSet<String> = HashSet::new();
    let mut from_str_targets: HashSet<String> = HashSet::new();

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
                    from_str_targets.insert(type_trimmed);
                }
            }
            _ => {}
        }
    }

    for (enum_node, name) in enums {
        if display_targets.contains(&name) && from_str_targets.contains(&name) {
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
}
