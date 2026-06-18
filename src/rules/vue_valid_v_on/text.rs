//! vue-valid-v-on AST backend.
//!
//! Walks `directive_attribute` nodes. For each `v-on` directive — long form
//! (`directive_name` is `v-on`) or `@` shorthand (`directive_name` is `@`) —
//! reports, in this order: a long-form directive with neither an event-name
//! argument nor a value (`MissingEventName`); the first modifier that is not a
//! known event/key/system modifier (`InvalidModifier`); a directive with no
//! handler value (`MissingHandler`). The `@` shorthand always carries an
//! argument, so it is never flagged for a missing event name. A long-form
//! directive with a value but no argument (`v-on="{ click: f }"` /
//! `v-on="listeners"`) is Vue's object syntax for binding multiple listeners and
//! is valid.

use crate::diagnostic::{Diagnostic, Severity};

const MSG_MISSING_EVENT_NAME: &str = "The v-on directive is missing an event name.";
const MSG_INVALID_MODIFIER: &str = "Invalid v-on modifier.";
const MSG_MISSING_HANDLER: &str =
    "The v-on directive for this event is missing a handler expression.";

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether a `directive_modifier` token names a known/allowed modifier. Single
/// characters (key codes), all-digit codes, known event/system modifiers and
/// key aliases are all allowed.
fn is_valid_modifier(text: &str) -> bool {
    text.len() == 1
        || VALID_MODIFIERS.contains(&text)
        || VALID_KEY_ALIASES.contains(&text)
        || text.bytes().all(|b| b.is_ascii_digit())
}

/// The first invalid `directive_modifier` node inside a directive's
/// `directive_modifiers` child, if any.
fn first_invalid_modifier<'a>(
    directive: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = directive.walk();
    let modifiers = directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_modifiers")?;
    let mut mod_cursor = modifiers.walk();
    modifiers.children(&mut mod_cursor).find(|m| {
        m.kind() == "directive_modifier"
            && m.utf8_text(source)
                .is_ok_and(|text| !is_valid_modifier(text))
    })
}

fn push(
    diagnostics: &mut Vec<Diagnostic>,
    node: tree_sitter::Node,
    ctx_path: &std::sync::Arc<std::path::Path>,
    message: &str,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(ctx_path),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-on", "@"] => |node, source, ctx, diagnostics|
    let name = directive_name(node, source);
    let is_shorthand = name == Some("@");
    if name != Some("v-on") && !is_shorthand {
        return;
    }

    let mut has_argument = false;
    let mut has_value = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "directive_argument" | "directive_dynamic_argument" => has_argument = true,
            "attribute_value" | "quoted_attribute_value" => has_value = true,
            _ => {}
        }
    }

    // The `@` shorthand always carries an argument; only the long form can be
    // missing its event name. A value with no argument is the object syntax
    // (`v-on="{ click: f }"`), where event names are keys in the bound object.
    if !is_shorthand && !has_argument && !has_value {
        push(diagnostics, node, &ctx.path_arc, MSG_MISSING_EVENT_NAME);
        return;
    }

    if let Some(invalid) = first_invalid_modifier(node, source) {
        push(diagnostics, invalid, &ctx.path_arc, MSG_INVALID_MODIFIER);
        return;
    }

    if !has_value {
        push(diagnostics, node, &ctx.path_arc, MSG_MISSING_HANDLER);
    }
}

/// Known event/system modifiers accepted on a `v-on` directive.
const VALID_MODIFIERS: &[&str] = &[
    "stop", "prevent", "capture", "self", "ctrl", "shift", "alt", "meta", "native", "once", "left",
    "right", "middle", "passive", "esc", "tab", "enter", "space", "up", "down", "delete", "exact",
];

/// Known key aliases (kebab-cased). Mirrors eslint-plugin-vue's key-aliases.
const VALID_KEY_ALIASES: &[&str] = &[
    "a-v-r-input",
    "a-v-r-power",
    "accept",
    "again",
    "all-candidates",
    "alphanumeric",
    "alt",
    "alt-graph",
    "app-switch",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    "arrow-up",
    "attn",
    "audio-balance-left",
    "audio-balance-right",
    "audio-bass-boost-down",
    "audio-bass-boost-toggle",
    "audio-bass-boost-up",
    "audio-fader-front",
    "audio-fader-rear",
    "audio-surround-mode-next",
    "audio-treble-down",
    "audio-treble-up",
    "audio-volume-down",
    "audio-volume-mute",
    "audio-volume-up",
    "backspace",
    "brightness-down",
    "brightness-up",
    "browser-back",
    "browser-favorites",
    "browser-forward",
    "browser-home",
    "browser-refresh",
    "browser-search",
    "browser-stop",
    "call",
    "camera",
    "camera-focus",
    "cancel",
    "caps-lock",
    "channel-down",
    "channel-up",
    "clear",
    "close",
    "closed-caption-toggle",
    "code-input",
    "color-f0-red",
    "color-f1-green",
    "color-f2-yellow",
    "color-f3-blue",
    "color-f4-grey",
    "color-f5-brown",
    "compose",
    "context-menu",
    "control",
    "convert",
    "copy",
    "cr-sel",
    "cut",
    "d-v-r",
    "dead",
    "delete",
    "dimmer",
    "display-swap",
    "eisu",
    "eject",
    "end",
    "end-call",
    "enter",
    "erase-eof",
    "escape",
    "ex-sel",
    "execute",
    "exit",
    "f1",
    "f10",
    "f11",
    "f12",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "favorite-clear0",
    "favorite-clear1",
    "favorite-clear2",
    "favorite-clear3",
    "favorite-recall0",
    "favorite-recall1",
    "favorite-recall2",
    "favorite-recall3",
    "favorite-store0",
    "favorite-store1",
    "favorite-store2",
    "favorite-store3",
    "final-mode",
    "find",
    "fn",
    "fn-lock",
    "go-back",
    "go-home",
    "group-first",
    "group-last",
    "group-next",
    "group-previous",
    "guide",
    "guide-next-day",
    "guide-previous-day",
    "hangul-mode",
    "hanja-mode",
    "hankaku",
    "headset-hook",
    "help",
    "hibernate",
    "hiragana",
    "hiragana-katakana",
    "home",
    "hyper",
    "info",
    "insert",
    "instant-replay",
    "junja-mode",
    "kana-mode",
    "kanji-mode",
    "katakana",
    "key11",
    "key12",
    "last-number-redial",
    "launch-application1",
    "launch-application2",
    "launch-calendar",
    "launch-contacts",
    "launch-mail",
    "launch-media-player",
    "launch-music-player",
    "launch-phone",
    "launch-screen-saver",
    "launch-spreadsheet",
    "launch-web-browser",
    "launch-web-cam",
    "launch-word-processor",
    "link",
    "list-program",
    "live-content",
    "lock",
    "log-off",
    "mail-forward",
    "mail-reply",
    "mail-send",
    "manner-mode",
    "media-apps",
    "media-close",
    "media-fast-forward",
    "media-last",
    "media-next-track",
    "media-pause",
    "media-play",
    "media-play-pause",
    "media-previous-track",
    "media-record",
    "media-rewind",
    "media-skip-backward",
    "media-skip-forward",
    "media-step-backward",
    "media-step-forward",
    "media-stop",
    "media-top-menu",
    "media-track-next",
    "media-track-previous",
    "meta",
    "microphone-toggle",
    "microphone-volume-down",
    "microphone-volume-mute",
    "microphone-volume-up",
    "mode-change",
    "navigate-in",
    "navigate-next",
    "navigate-out",
    "navigate-previous",
    "new",
    "next-candidate",
    "next-favorite-channel",
    "next-user-profile",
    "non-convert",
    "notification",
    "num-lock",
    "on-demand",
    "open",
    "page-down",
    "page-up",
    "pairing",
    "paste",
    "pause",
    "pin-p-down",
    "pin-p-move",
    "pin-p-toggle",
    "pin-p-up",
    "play-speed-down",
    "play-speed-reset",
    "play-speed-up",
    "power",
    "previous-candidate",
    "print",
    "print-screen",
    "process",
    "random-toggle",
    "rc-low-battery",
    "record-speed-next",
    "redo",
    "rf-bypass",
    "romaji",
    "s-t-b-input",
    "s-t-b-power",
    "save",
    "scan-channels-toggle",
    "screen-mode-next",
    "scroll-lock",
    "select",
    "settings",
    "shift",
    "single-candidate",
    "soft1",
    "soft2",
    "soft3",
    "soft4",
    "speech-correction-list",
    "speech-input-toggle",
    "spell-check",
    "split-screen-toggle",
    "standby",
    "subtitle",
    "super",
    "symbol",
    "symbol-lock",
    "t-v",
    "t-v-antenna-cable",
    "t-v-audio-description",
    "t-v-audio-description-mix-down",
    "t-v-audio-description-mix-up",
    "t-v-contents-menu",
    "t-v-data-service",
    "t-v-input",
    "t-v-input-component1",
    "t-v-input-component2",
    "t-v-input-composite1",
    "t-v-input-composite2",
    "t-v-input-h-d-m-i1",
    "t-v-input-h-d-m-i2",
    "t-v-input-h-d-m-i3",
    "t-v-input-h-d-m-i4",
    "t-v-input-v-g-a1",
    "t-v-media-context",
    "t-v-network",
    "t-v-number-entry",
    "t-v-power",
    "t-v-radio-service",
    "t-v-satellite",
    "t-v-satellite-b-s",
    "t-v-satellite-c-s",
    "t-v-satellite-toggle",
    "t-v-terrestrial-analog",
    "t-v-terrestrial-digital",
    "t-v-timer",
    "t-v3-d-mode",
    "tab",
    "teletext",
    "undo",
    "unidentified",
    "video-mode-next",
    "voice-dial",
    "wake-up",
    "wink",
    "zenkaku",
    "zenkaku-hankaku",
    "zoom-in",
    "zoom-out",
    "zoom-toggle",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    fn wrap(body: &str) -> String {
        format!("<template>\n{body}\n</template>")
    }

    // --- Invalid fixtures (Biome invalid.vue) ---

    #[test]
    fn flags_long_form_without_argument() {
        let diags = run(&wrap("<div v-on></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing an event name"));
    }

    #[test]
    fn allows_long_form_object_syntax_binding() {
        // `v-on="<object>"` is Vue's documented object syntax (#3756).
        assert!(run(&wrap("<div v-on=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn flags_long_form_missing_handler() {
        let diags = run(&wrap("<div v-on:click></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a handler"));
    }

    #[test]
    fn flags_shorthand_missing_handler() {
        let diags = run(&wrap("<div @click></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a handler"));
    }

    #[test]
    fn flags_invalid_modifier_long_form() {
        let diags = run(&wrap("<div v-on:click.bogus=\"foo\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_invalid_modifier_shorthand() {
        let diags = run(&wrap("<span @click.badModifier=\"foo\"></span>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_invalid_among_valid_modifiers() {
        // `stop` is valid, `wrong` is not.
        let diags = run(&wrap("<p @click.stop.wrong=\"foo\"></p>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_invalid_modifier_with_dynamic_argument() {
        let diags = run(&wrap("<p v-on:[event].notAValidModifier=\"foo\"></p>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_first_of_multiple_invalid_modifiers() {
        let diags = run(&wrap(
            "<button @submit.invalidModifier.anotherBad=\"handler\"></button>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_invalid_modifier_on_component() {
        let diags = run(&wrap(
            "<MyComponent @click.weird=\"someHandler\"></MyComponent>",
        ));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Invalid v-on modifier"));
    }

    #[test]
    fn flags_missing_handler_with_valid_modifier() {
        let diags = run(&wrap("<div @keyup.enter></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a handler"));
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<div v-on></div>\n\
             <div v-on:click></div>\n\
             <div @click></div>\n\
             <div v-on:click.bogus=\"foo\"></div>\n\
             <span @click.badModifier=\"foo\"></span>\n\
             <p @click.stop.wrong=\"foo\"></p>\n\
             <p v-on:[event].notAValidModifier=\"foo\"></p>\n\
             <button @submit.invalidModifier.anotherBad=\"handler\"></button>\n\
             <MyComponent @click.weird=\"someHandler\"></MyComponent>\n\
             <div @keyup.enter></div>",
        );
        assert_eq!(run(&source).len(), 10);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_long_form_handler() {
        assert!(run(&wrap("<div v-on:click=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_shorthand_handler() {
        assert!(run(&wrap("<div @click=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_valid_modifiers() {
        for modifier in ["stop", "prevent", "capture", "self", "once", "passive", "exact"] {
            let body = format!("<div v-on:click.{modifier}=\"foo\"></div>");
            assert!(run(&wrap(&body)).is_empty(), "modifier `{modifier}` should be valid");
        }
    }

    #[test]
    fn allows_system_and_mouse_modifiers() {
        for modifier in ["ctrl", "shift", "alt", "meta", "left", "right", "middle"] {
            let body = format!("<div @keydown.{modifier}=\"handler\"></div>");
            assert!(run(&wrap(&body)).is_empty(), "modifier `{modifier}` should be valid");
        }
    }

    #[test]
    fn allows_key_aliases() {
        for modifier in ["enter", "tab", "delete", "esc", "space", "up", "down", "left", "right", "arrow-down"] {
            let body = format!("<div @keydown.{modifier}=\"handler\"></div>");
            assert!(run(&wrap(&body)).is_empty(), "alias `{modifier}` should be valid");
        }
    }

    #[test]
    fn allows_single_char_and_numeric_modifiers() {
        assert!(run(&wrap("<div @keydown.a=\"handler\"></div>")).is_empty());
        assert!(run(&wrap("<div @keydown.b=\"handler\"></div>")).is_empty());
        assert!(run(&wrap("<div @keydown.a.b.c=\"handler\"></div>")).is_empty());
        assert!(run(&wrap("<div @keydown.27=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_combined_modifiers() {
        assert!(run(&wrap("<div @click.stop.prevent=\"foo\"></div>")).is_empty());
        assert!(run(&wrap("<div @keyup.enter.exact=\"onKey\"></div>")).is_empty());
    }

    #[test]
    fn allows_dynamic_argument() {
        assert!(run(&wrap("<div v-on:[event]=\"handler\"></div>")).is_empty());
        assert!(run(&wrap("<div @[event]=\"handler\"></div>")).is_empty());
    }

    #[test]
    fn allows_multiple_v_on_on_same_element() {
        assert!(run(&wrap("<div v-on:click=\"foo\" @mouseenter=\"bar\"></div>")).is_empty());
    }

    #[test]
    fn allows_component_event_handlers() {
        let diags = run(&wrap(
            "<MyComponent @click=\"handler\" @custom-event=\"customHandler\"></MyComponent>",
        ));
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_native_modifier() {
        assert!(run(&wrap("<div @click.native=\"handler\"></div>")).is_empty());
    }

    #[test]
    fn allows_object_syntax_literal() {
        // Object syntax binds multiple listeners without an argument (#3756).
        assert!(run(&wrap(
            "<button v-on=\"{ mousedown: onDown, mouseup: onUp }\">x</button>",
        ))
        .is_empty());
    }

    #[test]
    fn allows_object_syntax_variable() {
        // vee-validate `validationListeners` shape: object bound via a variable.
        assert!(run(&wrap(
            "<input :value=\"value\" v-on=\"validationListeners\" type=\"text\" />",
        ))
        .is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        let source = wrap(
            "<div v-on:click=\"foo\"></div>\n\
             <div @click=\"foo\"></div>\n\
             <div v-on:click.stop=\"foo\"></div>\n\
             <div v-on:click.prevent=\"foo\"></div>\n\
             <div v-on:click.capture=\"foo\"></div>\n\
             <div v-on:click.self=\"foo\"></div>\n\
             <div v-on:click.once=\"foo\"></div>\n\
             <div v-on:click.passive=\"foo\"></div>\n\
             <div v-on:click.exact=\"foo\"></div>\n\
             <div @keydown.ctrl=\"handler\"></div>\n\
             <div @keydown.shift=\"handler\"></div>\n\
             <div @keydown.alt=\"handler\"></div>\n\
             <div @keydown.meta=\"handler\"></div>\n\
             <div @mousedown.left=\"handler\"></div>\n\
             <div @mousedown.right=\"handler\"></div>\n\
             <div @mousedown.middle=\"handler\"></div>\n\
             <div @keydown.enter=\"handler\"></div>\n\
             <div @keydown.tab=\"handler\"></div>\n\
             <div @keydown.delete=\"handler\"></div>\n\
             <div @keydown.esc=\"handler\"></div>\n\
             <div @keydown.space=\"handler\"></div>\n\
             <div @keydown.up=\"handler\"></div>\n\
             <div @keydown.down=\"handler\"></div>\n\
             <div @keydown.left=\"handler\"></div>\n\
             <div @keydown.right=\"handler\"></div>\n\
             <div @keydown.arrow-down=\"handler\"></div>\n\
             <div @keydown.a=\"handler\"></div>\n\
             <div @keydown.b=\"handler\"></div>\n\
             <div @keydown.a.b.c=\"handler\"></div>\n\
             <div @keydown.27=\"foo\"></div>\n\
             <div @click.stop.prevent=\"foo\"></div>\n\
             <div @keyup.enter.exact=\"onKey\"></div>\n\
             <div v-on:[event]=\"handler\"></div>\n\
             <div @[event]=\"handler\"></div>\n\
             <div v-on:click=\"foo\" @mouseenter=\"bar\"></div>\n\
             <MyComponent @click=\"handler\" @custom-event=\"customHandler\"></MyComponent>\n\
             <div @click.native=\"handler\"></div>",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" :id=\"x\" v-bind:class=\"c\"></div>")).is_empty());
    }

    #[test]
    fn ignores_v_bind_shorthand() {
        // `:id` is a v-bind shorthand (directive_name `:`), not a v-on.
        assert!(run(&wrap("<div :id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn allows_single_quoted_handler() {
        assert!(run(&wrap("<div @click='foo'></div>")).is_empty());
    }
}
