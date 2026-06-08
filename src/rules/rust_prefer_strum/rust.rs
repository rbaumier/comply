//! rust-prefer-strum backend.
//!
//! On each `source_file` node, scans direct children for:
//! 1. `enum_item` nodes — collect their names.
//! 2. `impl_item` nodes whose `trait` text matches a `Display` path —
//!    record the target type name.
//! 3. `impl_item` nodes whose `trait` text matches a `FromStr` path —
//!    record the target type name.
//!
//! Any enum name present in BOTH (2) and (3) is flagged on its
//! `enum_item` node. The two impls together are the redundancy
//! `strum::Display` + `strum::EnumString` is designed to remove.

use rustc_hash::FxHashSet;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let mut enums: Vec<(tree_sitter::Node, String)> = Vec::new();
    let mut display_targets: FxHashSet<String> = FxHashSet::default();
    let mut from_str_targets: FxHashSet<String> = FxHashSet::default();

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "enum_item" => {
                if let Some(name_node) = child.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
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
                } else if is_from_str_trait(trait_trimmed) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
