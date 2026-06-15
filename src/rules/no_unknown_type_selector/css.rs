use crate::diagnostic::{Diagnostic, Severity};

/// HTML element names (matched case-insensitively). Sorted for `binary_search`.
const HTML_TAGS: &[&str] = &[
    "a", "abbr", "acronym", "address", "applet", "area", "article", "aside", "audio", "b", "base",
    "basefont", "bdi", "bdo", "bgsound", "big", "blink", "blockquote", "body", "br", "button",
    "canvas", "caption", "center", "cite", "code", "col", "colgroup", "content", "data",
    "datalist", "dd", "del", "details", "dfn", "dialog", "dir", "div", "dl", "dt", "em", "embed",
    "fencedframe", "fieldset", "figcaption", "figure", "font", "footer", "form", "frame",
    "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html", "i",
    "iframe", "img", "input", "ins", "isindex", "kbd", "keygen", "label", "legend", "li", "link",
    "listbox", "listing", "main", "map", "mark", "marquee", "math", "menu", "menuitem", "meta",
    "meter", "model", "multicol", "nav", "nextid", "nobr", "noembed", "noframes", "noscript",
    "object", "ol", "optgroup", "option", "output", "p", "param", "picture", "plaintext", "portal",
    "pre", "progress", "q", "rb", "rp", "rt", "rtc", "ruby", "s", "samp", "script", "search",
    "section", "select", "selectlist", "shadow", "slot", "small", "source", "spacer", "span",
    "strike", "strong", "style", "sub", "summary", "sup", "svg", "table", "tbody", "td", "template",
    "textarea", "tfoot", "th", "thead", "time", "title", "tr", "track", "tt", "u", "ul", "var",
    "video", "wbr", "xmp",
];

/// SVG element names (matched case-sensitively — SVG has camelCase names like
/// `linearGradient`). Sorted for `binary_search`.
const SVG_TAGS: &[&str] = &[
    "a", "altGlyph", "altGlyphDef", "altGlyphItem", "animate", "animateColor", "animateMotion",
    "animateTransform", "circle", "clipPath", "color-profile", "cursor", "defs", "desc", "ellipse",
    "feBlend", "feColorMatrix", "feComponentTransfer", "feComposite", "feConvolveMatrix",
    "feDiffuseLighting", "feDisplacementMap", "feDistantLight", "feDropShadow", "feFlood",
    "feFuncA", "feFuncB", "feFuncG", "feFuncR", "feGaussianBlur", "feImage", "feMerge",
    "feMergeNode", "feMorphology", "feOffset", "fePointLight", "feSpecularLighting", "feSpotLight",
    "feTile", "feTurbulence", "filter", "font", "font-face", "font-face-format", "font-face-name",
    "font-face-src", "font-face-uri", "foreignObject", "g", "glyph", "glyphRef", "hatch",
    "hatchpath", "hkern", "image", "line", "linearGradient", "marker", "mask", "metadata",
    "missing-glyph", "mpath", "path", "pattern", "polygon", "polyline", "radialGradient", "rect",
    "script", "set", "stop", "style", "svg", "switch", "symbol", "text", "textPath", "title",
    "tspan", "use", "view", "vkern",
];

/// MathML element names (matched case-insensitively). Sorted for `binary_search`.
const MATH_ML_TAGS: &[&str] = &[
    "annotation", "annotation-xml", "maction", "math", "menclose", "merror", "mfenced", "mfrac",
    "mi", "mmultiscripts", "mn", "mo", "mover", "mpadded", "mphantom", "mprescripts", "mroot",
    "mrow", "ms", "mspace", "msqrt", "mstyle", "msub", "msubsup", "msup", "mtable", "mtd", "mtext",
    "mtr", "munder", "munderover", "semantics",
];

/// Pseudo-element functions whose `root` argument is a valid keyword, not a type
/// selector to validate (e.g. `::view-transition-old(root)`).
const VIEW_TRANSITION_PSEUDO_ELEMENTS: &[&str] = &[
    "view-transition",
    "view-transition-group",
    "view-transition-image-pair",
    "view-transition-old",
    "view-transition-new",
];

/// A name is a custom element if it contains a hyphen and is already lowercase
/// (so `x-foo` is valid but `x-Foo` is not).
fn is_custom_element(name: &str) -> bool {
    name.contains('-') && name == name.to_lowercase()
}

fn is_known_type_selector(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    HTML_TAGS.binary_search(&lower.as_str()).is_ok()
        || SVG_TAGS.binary_search(&name).is_ok()
        || MATH_ML_TAGS.binary_search(&lower.as_str()).is_ok()
        || is_custom_element(name)
}

/// Decide whether a `tag_name` node is a genuine CSS type selector (an
/// element-name selector). tree-sitter-css also produces `tag_name` nodes for
/// namespace prefixes (`svg|rect`) and pseudo-element names (`a::before`),
/// which are not type selectors and must not be validated.
fn is_type_selector(node: &tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "selectors"
        | "class_selector"
        | "id_selector"
        | "attribute_selector"
        | "pseudo_class_selector"
        | "descendant_selector"
        | "child_selector"
        | "sibling_selector"
        | "adjacent_sibling_selector"
        | "arguments" => true,
        // `svg|rect`: the element follows the `|` token; the namespace prefix
        // precedes it and is not a type selector.
        "namespace_selector" => after_token(&parent, node, "|"),
        // `a::before`: the leading element precedes `::`; the pseudo-element
        // name follows it and is not a type selector.
        "pseudo_element_selector" => !after_token(&parent, node, "::"),
        _ => false,
    }
}

/// Whether `node` starts after the first child of `parent` whose kind equals
/// `token`. Returns `false` when the token is absent.
fn after_token(parent: &tree_sitter::Node, node: &tree_sitter::Node, token: &str) -> bool {
    let mut cursor = parent.walk();
    let sep = parent
        .children(&mut cursor)
        .find(|child| child.kind() == token);
    match sep {
        Some(sep) => node.start_byte() > sep.start_byte(),
        None => false,
    }
}

/// `root` inside a view-transition pseudo-element function is a keyword, not a
/// type selector (e.g. `::view-transition-old(root)`).
fn is_root_in_view_transition(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.utf8_text(source).unwrap_or_default() != "root" {
        return false;
    }
    let Some(arguments) = node.parent() else {
        return false;
    };
    if arguments.kind() != "arguments" {
        return false;
    }
    let Some(pseudo) = arguments.parent() else {
        return false;
    };
    if pseudo.kind() != "pseudo_element_selector" {
        return false;
    }
    // The pseudo-element name is the `tag_name` after the `::` token.
    let mut cursor = pseudo.walk();
    pseudo
        .children(&mut cursor)
        .filter(|child| child.kind() == "tag_name" && after_token(&pseudo, child, "::"))
        .any(|name| {
            let text = name.utf8_text(source).unwrap_or_default();
            VIEW_TRANSITION_PSEUDO_ELEMENTS.contains(&text)
        })
}

crate::ast_check! { on ["tag_name"] => |node, source, ctx, diagnostics|
    if !is_type_selector(&node) {
        return;
    }
    let name = node.utf8_text(source).unwrap_or_default();
    if is_known_type_selector(name) {
        return;
    }
    if is_root_in_view_transition(&node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Unknown type selector `{name}`."),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // --- Biome `valid.css` fixtures: must not fire. ---

    #[test]
    fn allows_known_html_element() {
        assert!(run("input {\n}").is_empty());
    }

    #[test]
    fn allows_descendant_of_known_elements() {
        assert!(run("ul li {\n}").is_empty());
    }

    #[test]
    fn allows_child_combinator_of_known_elements() {
        assert!(run("li > a {\n}").is_empty());
    }

    #[test]
    fn allows_selector_list_of_known_elements() {
        assert!(run("table,\ntr {\n}").is_empty());
    }

    #[test]
    fn allows_custom_element_with_hyphen() {
        assert!(run("x-foo {\n}").is_empty());
    }

    #[test]
    fn allows_known_svg_element() {
        assert!(run("g {\n}").is_empty());
    }

    #[test]
    fn allows_known_mathml_element() {
        assert!(run("mfrac {\n}").is_empty());
    }

    #[test]
    fn allows_root_in_view_transition_pseudo_elements() {
        assert!(
            run("::view-transition-old(root),\n::view-transition-new(root) {\n\tz-index: 1;\n}")
                .is_empty()
        );
    }

    // --- Biome `invalid.css` fixtures: must fire once each. ---

    #[test]
    fn flags_unknown_type_selector() {
        assert_eq!(run("unknown {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_in_descendant_subject() {
        assert_eq!(run("ul unknown {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_as_descendant_ancestor() {
        assert_eq!(run("unknown ul {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_after_child_combinator() {
        assert_eq!(run("li > hoge {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_before_child_combinator() {
        assert_eq!(run("fuga > li {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_in_selector_list_second() {
        assert_eq!(run("table,\nunknown {\n}").len(), 1);
    }

    #[test]
    fn flags_unknown_in_selector_list_first() {
        assert_eq!(run("unknown,\narticle {\n}").len(), 1);
    }

    #[test]
    fn flags_pseudo_custom_element_with_uppercase() {
        // `x-Foo` has a hyphen but is not lowercase, so it is not a custom element.
        assert_eq!(run("x-Foo {\n}").len(), 1);
    }

    // --- Additional coverage for the tree-sitter-css node shapes. ---

    #[test]
    fn allows_class_selector() {
        assert!(run(".foo {\n}").is_empty());
    }

    #[test]
    fn allows_id_selector() {
        assert!(run("#bar {\n}").is_empty());
    }

    #[test]
    fn allows_pseudo_class_on_known_element() {
        assert!(run("a:hover {\n}").is_empty());
    }

    #[test]
    fn allows_pseudo_element_on_known_element() {
        // `before` is the pseudo-element name, not a type selector.
        assert!(run("a::before {\n}").is_empty());
    }

    #[test]
    fn allows_namespaced_known_element() {
        // The `svg` namespace prefix must not be treated as a type selector.
        assert!(run("svg|rect {\n}").is_empty());
    }

    #[test]
    fn flags_namespaced_unknown_element() {
        assert_eq!(run("svg|unknown {\n}").len(), 1);
    }

    #[test]
    fn allows_uppercase_html_element() {
        // HTML element names are matched case-insensitively.
        assert!(run("INPUT {\n}").is_empty());
    }

    #[test]
    fn allows_attribute_selector_on_known_element() {
        assert!(run("input[type=\"text\"] {\n}").is_empty());
    }
}
