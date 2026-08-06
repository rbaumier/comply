//! Report the layout properties an `@keyframes` interpolates.
//!
//! The unit of the finding is `(@keyframes block, property)`. Whether a
//! property animates is a fact about the block, not about one declaration.
//! Each property is therefore reported once, at its first declaration.
//!
//! A property is reported only when it interpolates. It does not interpolate
//! when it carries one value at every offset it appears in. Those offsets must
//! also pin both ends of the timeline. The computed value then holds from the
//! first frame to the last, and nothing costs layout.
//!
//! A value that reads a custom property moves when that custom property moves.
//! The custom properties of the block are accumulated the same way. Their
//! movement then travels to the values that read them.

use crate::diagnostic::{Diagnostic, Severity};

/// Properties whose interpolation makes the browser recompute layout on every
/// frame.
const LAYOUT_PROPERTIES: &[&str] = &[
    "top",
    "left",
    "right",
    "bottom",
    "width",
    "height",
    "margin",
    "margin-top",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "padding",
    "padding-top",
    "padding-bottom",
    "padding-left",
    "padding-right",
];

/// Which ends of the timeline a set of keyframe offsets holds down.
#[derive(Clone, Copy, Default)]
struct Pins {
    start: bool,
    end: bool,
}

/// One property, accumulated over the offsets of a single `@keyframes`.
struct Animated<'t, 's> {
    /// A layout property under the spelling of `LAYOUT_PROPERTIES`, or a custom
    /// property under its own — the case of a custom property is significant.
    property: &'s str,
    /// The property name of the first declaration. A layout property reports
    /// its finding there.
    site: tree_sitter::Node<'t>,
    first_value: String,
    varies: bool,
    pins: Pins,
}

impl Animated<'_, '_> {
    /// An end that no offset pins falls back to the element's computed value.
    /// The stylesheet cannot see that value, so the property counts as
    /// animated.
    fn interpolates(&self) -> bool {
        self.varies || !(self.pins.start && self.pins.end)
    }
}

crate::ast_check! { on ["keyframes_statement"] => |node, source, ctx, diagnostics|
    let mut animated: Vec<Animated> = Vec::new();

    for (offsets, body) in keyframe_blocks(node, source) {
        let pins = pinned_ends(offsets);
        let mut cursor = body.walk();
        for declaration in body.children(&mut cursor) {
            if declaration.kind() != "declaration" { continue; }
            let Some((property, site, value)) = tracked_declaration(declaration, source) else {
                continue;
            };
            match animated.iter_mut().find(|seen| seen.property == property) {
                Some(seen) => {
                    seen.varies |= seen.first_value != value;
                    seen.pins.start |= pins.start;
                    seen.pins.end |= pins.end;
                }
                None => animated.push(Animated {
                    property,
                    site,
                    first_value: value,
                    varies: false,
                    pins,
                }),
            }
        }
    }

    spread_custom_property_movement(&mut animated);

    for seen in animated
        .iter()
        .filter(|seen| !is_custom_property(seen.property) && seen.interpolates())
    {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &seen.site,
            super::META.id,
            format!(
                "`@keyframes` animates `{}` — prefer animating `transform`/`opacity` to stay off the layout/paint path.",
                seen.property
            ),
            Severity::Error,
        ));
    }
}

/// The body of every keyframe block in a `@keyframes`, paired with the offsets
/// declared in front of it.
///
/// tree-sitter-css has no production for a comma-separated offset list.
/// `0%, 100% { … }` parses as an `ERROR` node holding `0%`, then a
/// `keyframe_block` whose only offset node is `100%`. Reading the source
/// between the end of one block and the start of the next block's body
/// recovers the offsets the grammar drops.
fn keyframe_blocks<'t, 's>(
    keyframes: tree_sitter::Node<'t>,
    source: &'s [u8],
) -> Vec<(&'s str, tree_sitter::Node<'t>)> {
    let mut blocks = Vec::new();
    let mut keyframes_cursor = keyframes.walk();
    for list in keyframes.children(&mut keyframes_cursor) {
        if list.kind() != "keyframe_block_list" {
            continue;
        }
        let mut offsets_start = list.start_byte();
        let mut list_cursor = list.walk();
        for child in list.children(&mut list_cursor) {
            if child.kind() == "{" {
                offsets_start = child.end_byte();
                continue;
            }
            if child.kind() != "keyframe_block" {
                continue;
            }
            let mut block_cursor = child.walk();
            let body = child.children(&mut block_cursor).find(|c| c.kind() == "block");
            let offsets_from = offsets_start;
            offsets_start = child.end_byte();
            let Some(body) = body else { continue };
            let offsets = source
                .get(offsets_from..body.start_byte())
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .unwrap_or_default();
            blocks.push((offsets, body));
        }
    }
    blocks
}

/// CSS reserves the `--` prefix for custom properties.
fn is_custom_property(name: &str) -> bool {
    name.starts_with("--")
}

/// Mark every property whose value reads a custom property that moves.
///
/// A custom property may be declared after the value that reads it. It may
/// itself read another, so the movement travels once the whole `@keyframes` is
/// accumulated. A custom property joins the frontier when it starts moving,
/// which happens at most once, so the frontier empties.
fn spread_custom_property_movement<'s>(animated: &mut [Animated<'_, 's>]) {
    let mut frontier: Vec<&'s str> = animated
        .iter()
        .filter(|seen| is_custom_property(seen.property) && seen.interpolates())
        .map(|seen| seen.property)
        .collect();
    while !frontier.is_empty() {
        let mut moved = Vec::new();
        for seen in animated.iter_mut().filter(|seen| !seen.interpolates()) {
            if frontier.iter().any(|name| reads_custom_property(&seen.first_value, name)) {
                seen.varies = true;
                if is_custom_property(seen.property) {
                    moved.push(seen.property);
                }
            }
        }
        frontier = moved;
    }
}

/// Whether the value names this custom property, rather than one that merely
/// contains its name: `--w` is neither `var(--width)` nor `var(--x--w)`.
fn reads_custom_property(value: &str, name: &str) -> bool {
    value.match_indices(name).any(|(at, _)| {
        let before = value[..at].chars().next_back();
        let after = value[at + name.len()..].chars().next();
        !before.is_some_and(is_name_character) && !after.is_some_and(is_name_character)
    })
}

/// The characters a CSS identifier continues with.
fn is_name_character(c: char) -> bool {
    c == '-' || c == '_' || c.is_alphanumeric()
}

/// `from` and `0%` hold down the start of the timeline, `to` and `100%` the
/// end. An offset this parser cannot read pins neither end, so the properties
/// declared under it stay reportable.
fn pinned_ends(offsets: &str) -> Pins {
    let offsets = without_comments(offsets);
    let mut pins = Pins::default();
    for offset in offsets.split(',') {
        match offset_percent(offset) {
            Some(percent) if percent == 0.0 => pins.start = true,
            Some(percent) if percent == 100.0 => pins.end = true,
            _ => {}
        }
    }
    pins
}

/// The text with its `/* … */` spans blanked out. A comment beside an offset
/// makes that offset unreadable. A comment inside a value makes two spellings
/// of one value differ.
fn without_comments(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("/*") {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut stripped = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("/*") {
        stripped.push_str(before);
        stripped.push(' ');
        let Some((_, tail)) = after.split_once("*/") else {
            return std::borrow::Cow::Owned(stripped);
        };
        rest = tail;
    }
    stripped.push_str(rest);
    std::borrow::Cow::Owned(stripped)
}

fn offset_percent(offset: &str) -> Option<f64> {
    let offset = offset.trim();
    if offset.eq_ignore_ascii_case("from") {
        return Some(0.0);
    }
    if offset.eq_ignore_ascii_case("to") {
        return Some(100.0);
    }
    offset.strip_suffix('%')?.parse().ok()
}

/// The property name, the node to report it on, and the declared value — for
/// the layout properties the rule reports, and for every custom property the
/// block declares.
///
/// Whitespace runs and comments are normalised away. The rest of the value
/// compares verbatim: `var(--Foo)` and `var(--foo)` read two different custom
/// properties, so folding case would call two different values identical.
fn tracked_declaration<'t, 's>(
    declaration: tree_sitter::Node<'t>,
    source: &'s [u8],
) -> Option<(&'s str, tree_sitter::Node<'t>, String)> {
    let mut cursor = declaration.walk();
    let mut children = declaration.children(&mut cursor);

    let site = children.by_ref().find(|c| c.kind() == "property_name")?;
    let name = site.utf8_text(source).ok()?;
    // Layout property names are case-insensitive, so they fold to one spelling.
    let property = if is_custom_property(name) {
        name
    } else {
        LAYOUT_PROPERTIES.iter().copied().find(|p| p.eq_ignore_ascii_case(name))?
    };

    let separator = children.by_ref().find(|c| c.kind() == ":")?;
    let mut value_end = separator.end_byte();
    for child in children {
        if child.kind() == ";" {
            break;
        }
        value_end = child.end_byte();
    }
    let value = source
        .get(separator.end_byte()..value_end)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())?;
    let value = without_comments(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some((property, site, value))
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.css")
    }

    /// The property each finding names, in report order.
    fn properties(source: &str) -> Vec<String> {
        run(source)
            .iter()
            .map(|d| {
                let (_, quoted) = d
                    .message
                    .split_once("animates `")
                    .expect("the message names the property it reports");
                let (property, _) = quoted.split_once('`').expect("the property name is quoted");
                property.to_string()
            })
            .collect()
    }

    /// The byte offset each finding points at, in report order.
    fn reported_offsets(source: &str) -> Vec<usize> {
        run(source)
            .iter()
            .map(|d| d.span.expect("findings span the property name").0)
            .collect()
    }

    fn assert_clear(source: &str) {
        let found = run(source);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn flags_top_in_keyframes() {
        let css = "@keyframes slide { from { top: 0; } to { top: 100px; } }";
        assert_eq!(properties(css), ["top"]);
    }

    #[test]
    fn flags_width_in_keyframes() {
        let css = "@keyframes grow { from { width: 0; } to { width: 100px; } }";
        assert_eq!(properties(css), ["width"]);
    }

    #[test]
    fn allows_transform_and_opacity() {
        let css = "@keyframes fade { from { opacity: 0; transform: translateY(-10px); } to { opacity: 1; transform: none; } }";
        assert_clear(css);
    }

    #[test]
    fn allows_top_outside_keyframes() {
        assert_clear(".hero { top: 0; }");
    }

    #[test]
    fn allows_an_empty_keyframes() {
        assert_clear("@keyframes nothing { }");
    }

    /// nuxt/ui `src/runtime/keyframes.css:419` — the indeterminate Progress
    /// bar. `width` is declared once, under an offset list that covers both
    /// ends, so it holds `50%` for the whole animation and only `transform`
    /// moves.
    #[test]
    fn allows_constant_width_declared_at_both_ends_of_one_offset_list() {
        let css = "@keyframes carousel {\n  0%,\n  100% {\n    width: 50%;\n  }\n\n  0% {\n    transform: translateX(-100%);\n  }\n\n  100% {\n    transform: translateX(200%);\n  }\n}";
        assert_clear(css);
    }

    /// A comment beside an offset must not swallow it and leave the timeline
    /// looking half-pinned.
    #[test]
    fn allows_constant_width_when_comments_sit_between_the_offsets() {
        let css = "@keyframes carousel {\n  /* the constant */\n  0% /* mid */,\n  100% {\n    width: 50%;\n  }\n\n  0% {\n    transform: translateX(-100%);\n  }\n}";
        assert_clear(css);
    }

    /// A comment cannot forge a pin: its text is dropped, not handed to the
    /// offset parser.
    #[test]
    fn flags_width_when_only_a_comment_mentions_the_missing_end() {
        let css = "@keyframes k { /* 100%, */ 0% { width: 50%; } }";
        assert_eq!(properties(css), ["width"]);
    }

    #[test]
    fn allows_same_value_repeated_at_both_ends() {
        assert_clear("@keyframes k { 0% { width: 50%; } 100% { width: 50%; } }");
    }

    #[test]
    fn flags_different_values_at_the_two_ends() {
        let css = "@keyframes k { 0% { width: 50%; } 100% { width: 60%; } }";
        assert_eq!(properties(css), ["width"]);
    }

    /// An end no offset pins falls back to the element's computed width, which
    /// the stylesheet cannot see, so `width` may well interpolate.
    #[test]
    fn flags_single_value_that_pins_only_one_end() {
        assert_eq!(properties("@keyframes k { to { width: 100%; } }"), ["width"]);
        assert_eq!(properties("@keyframes k { 50% { width: 100%; } }"), ["width"]);
    }

    /// Folding case would make these two custom properties compare equal and
    /// wrongly clear the animation.
    #[test]
    fn flags_values_differing_only_by_the_case_of_a_custom_property() {
        let css = "@keyframes k { from { height: var(--Foo); } to { height: var(--foo); } }";
        assert_eq!(properties(css), ["height"]);
    }

    #[test]
    fn ignores_whitespace_when_comparing_values() {
        assert_clear("@keyframes k { from { margin: 0   10px; } to { margin: 0 10px; } }");
        assert_clear("@keyframes k { from { width: calc(1px  +  2px); } to { width: calc(1px + 2px); } }");
    }

    #[test]
    fn ignores_a_comment_inside_a_value() {
        assert_clear("@keyframes k { from { width: 5px; } to { width: /* c */ 5px; } }");
    }

    /// The text repeats but `--w` does not, so `width` still interpolates.
    #[test]
    fn flags_a_value_that_reads_a_custom_property_the_block_declares() {
        let css = "@keyframes k { from { width: var(--w); --w: 0px; } to { width: var(--w); --w: 100px; } }";
        assert_eq!(properties(css), ["width"]);
    }

    /// `--w` is not `--width-of-thing`; the block animates neither.
    #[test]
    fn allows_a_value_reading_a_custom_property_the_block_does_not_declare() {
        let css = "@keyframes k { from { width: var(--width-of-thing); --w: 0px; } to { width: var(--width-of-thing); --w: 100px; } }";
        assert_clear(css);
    }

    /// `--w` is not the `--x--w` this value reads either.
    #[test]
    fn allows_a_value_reading_a_custom_property_whose_name_ends_the_same_way() {
        let css = "@keyframes k { from { width: var(--x--w); --w: 0px; } to { width: var(--x--w); --w: 100px; } }";
        assert_clear(css);
    }

    /// `--w` holds one value across both ends, so `width` holds still with it.
    #[test]
    fn allows_a_value_reading_a_custom_property_that_holds_still() {
        let css = "@keyframes k { from { width: var(--w); --w: 5px; } to { width: var(--w); --w: 5px; } }";
        assert_clear(css);
    }

    /// `--outer` reads `--inner`, which moves. The movement reaches `width`
    /// through both of them.
    #[test]
    fn flags_a_value_that_reads_a_chain_of_custom_properties() {
        let css = "@keyframes k { from { width: var(--outer); --outer: var(--inner); --inner: 0px; } to { width: var(--outer); --outer: var(--inner); --inner: 100px; } }";
        assert_eq!(properties(css), ["width"]);
    }

    /// A block's last declaration may carry no `;`. Its value must still be
    /// read to the end, or two different values would compare equal.
    #[test]
    fn reads_a_declaration_that_has_no_trailing_semicolon() {
        assert_clear("@keyframes k { 0% { width: 50% } 100% { width: 50% } }");
        assert_eq!(
            properties("@keyframes k { 0% { width: 50% } 100% { width: 60% } }"),
            ["width"]
        );
    }

    /// `0 %` is not an offset, so it pins nothing and `width` stays reported.
    /// Neither is `0/*x*/%`: the comment leaves a gap the parser refuses.
    #[test]
    fn flags_width_when_an_offset_is_malformed() {
        let css = "@keyframes k { 0 % { width: 50%; } 100% { width: 50%; } }";
        assert_eq!(properties(css), ["width"]);
        let commented = "@keyframes k { 0/*x*/% { width: 50%; } 100% { width: 50%; } }";
        assert_eq!(properties(commented), ["width"]);
    }

    /// nuxt/ui `src/runtime/keyframes.css:541` — `elastic`. Two properties
    /// interpolate across three offsets; each is reported once, at its first
    /// declaration.
    #[test]
    fn reports_each_animated_property_once_per_block() {
        let css = "@keyframes elastic { 0%, 100% { width: 50%; left: 25%; } 50% { width: 100%; left: 0; } }";
        assert_eq!(properties(css), ["width", "left"]);
        assert_eq!(
            reported_offsets(css)[0],
            css.find("width").expect("the fixture declares width")
        );
    }

    /// nuxt/ui `src/runtime/keyframes.css:1` — the Reka UI accordion. `height`
    /// runs between two different values, so it does interpolate and stays
    /// reported; the `overflow: hidden` beside it changes nothing.
    #[test]
    fn flags_height_that_runs_between_two_values_next_to_overflow_hidden() {
        let css = "@keyframes accordion-down { from { height: 0; overflow: hidden; } to { height: var(--reka-accordion-content-height); overflow: hidden; } }";
        assert_eq!(properties(css), ["height"]);
    }

    /// Semantic-UI `components/progress.css:244` is `@-webkit-keyframes`, and
    /// the check keys on the `keyframes_statement` node rather than on the
    /// declaration.
    #[test]
    fn flags_a_vendor_prefixed_keyframes() {
        let css = "@-webkit-keyframes k { 0% { width: 0; } 100% { width: 9px; } }";
        assert_eq!(properties(css), ["width"]);
    }

    #[test]
    fn flags_keyframes_nested_in_a_media_query() {
        let css = "@media (min-width: 10px) { @keyframes k { from { height: 0; } to { height: 9px; } } }";
        assert_eq!(properties(css), ["height"]);
    }

    #[test]
    fn keeps_each_keyframes_block_separate() {
        let css = "@keyframes a { 0%, 100% { width: 50%; } } @keyframes b { from { width: 0; } to { width: 100%; } }";
        assert_eq!(properties(css), ["width"]);
        let second_block = css.find("@keyframes b").expect("the fixture has a second block");
        assert!(reported_offsets(css)[0] > second_block);
    }
}
