use crate::diagnostic::{Diagnostic, Severity};

const KNOWN: &[&str] = &[
    "calc",
    "var",
    "min",
    "max",
    "clamp",
    "env",
    "url",
    "attr",
    "counter",
    "counters",
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "hwb",
    "lab",
    "lch",
    "oklch",
    "oklab",
    "color",
    "color-mix",
    "linear-gradient",
    "radial-gradient",
    "conic-gradient",
    "repeating-linear-gradient",
    "repeating-radial-gradient",
    "repeating-conic-gradient",
    "image-set",
    "cross-fade",
    "element",
    "paint",
    "fit-content",
    "minmax",
    "repeat",
    "cubic-bezier",
    "steps",
    "linear",
    "ease",
    "path",
    "polygon",
    "circle",
    "ellipse",
    "inset",
    "translate",
    "translatex",
    "translatey",
    "translatez",
    "translate3d",
    "rotate",
    "rotatex",
    "rotatey",
    "rotatez",
    "rotate3d",
    "scale",
    "scalex",
    "scaley",
    "scalez",
    "scale3d",
    "skew",
    "skewx",
    "skewy",
    "matrix",
    "matrix3d",
    "perspective",
    "blur",
    "brightness",
    "contrast",
    "drop-shadow",
    "grayscale",
    "hue-rotate",
    "invert",
    "opacity",
    "saturate",
    "sepia",
    "format",
    "local",
    "symbols",
    "stylistic",
    "styleset",
    "character-variant",
    "swash",
    "ornaments",
    "annotation",
    "light-dark",
    "abs",
    "acos",
    "asin",
    "atan",
    "atan2",
    "cos",
    "exp",
    "hypot",
    "log",
    "mod",
    "pow",
    "rem",
    "round",
    "sign",
    "sin",
    "sqrt",
    "tan",
    "anchor",
    "anchor-size",
    "ray",
    "scroll",
    "view",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(name_node) = node.children(&mut c).find(|n| n.kind() == "function_name") else { return; };
    let name = name_node.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if name.starts_with("-webkit-") || name.starts_with("-moz-") || name.starts_with("-ms-") || name.starts_with("-o-") {
        return;
    }
    if KNOWN.iter().any(|k| *k == name) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Unknown CSS function `{name}`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_unknown_function() {
        assert_eq!(run(".a { width: unknown-func(10px); }").len(), 1);
    }

    #[test]
    fn allows_known_function() {
        assert!(run(".a { width: calc(100% - 10px); }").is_empty());
    }
}
