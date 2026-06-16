//! Port of Biome `noDescendingSpecificity`.
//!
//! Groups selectors by their tail (the rightmost compound) and flags a
//! selector whose specificity is strictly lower than an earlier selector with
//! the same tail. Specificity is the `(id, class, type)` triple defined by the
//! CSS spec; `:where()` contributes nothing and `:is()/:not()/:has()/:matches()`
//! contribute the most specific of their arguments.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashMap;

/// CSS specificity as the `(a, b, c)` triple: `a` = ids, `b` = classes /
/// attributes / pseudo-classes, `c` = types / pseudo-elements.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct Specificity(u32, u32, u32);

impl std::ops::Add for Specificity {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0, self.1 + rhs.1, self.2 + rhs.2)
    }
}

impl std::fmt::Display for Specificity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.0, self.1, self.2)
    }
}

const ID: Specificity = Specificity(1, 0, 0);
const CLASS: Specificity = Specificity(0, 1, 0);
const TYPE: Specificity = Specificity(0, 0, 1);
const ZERO: Specificity = Specificity(0, 0, 0);

/// Base specificity of a functional pseudo-class by name, mirroring the CSS
/// "is/not/has and nesting" exceptions. `None` means the whole functional
/// pseudo (and its arguments) contributes nothing (`:where()`).
fn pseudo_function_base(name: &str) -> Option<Specificity> {
    match name {
        "where" => None,
        "is" | "not" | "has" | "matches" => Some(ZERO),
        _ => Some(CLASS),
    }
}

fn named_children<'t>(node: tree_sitter::Node<'t>) -> Vec<tree_sitter::Node<'t>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}

/// Specificity of a single selector subtree.
fn specificity_of(node: tree_sitter::Node<'_>, source: &[u8]) -> Specificity {
    match node.kind() {
        // Combinators: sum the specificity of each operand.
        "descendant_selector"
        | "child_selector"
        | "sibling_selector"
        | "adjacent_sibling_selector"
        | "namespace_selector" => named_children(node)
            .into_iter()
            .map(|c| specificity_of(c, source))
            .fold(ZERO, |acc, s| acc + s),

        // Each `id_selector` level adds one id; `#a#b` nests one inside another.
        "id_selector" => named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "id_selector")
            .map(|c| specificity_of(c, source))
            .fold(ID, |acc, s| acc + s),

        "class_selector" | "attribute_selector" => CLASS,
        "tag_name" => TYPE,
        "universal_selector" => ZERO,
        // The CSS spec gives `&` no specificity, but Biome's CSS model scores
        // the nesting selector like a type selector, so we match Biome.
        "nesting_selector" => TYPE,

        // `el::after` — the leading element keeps its specificity and the
        // pseudo-element adds one type unit.
        "pseudo_element_selector" => {
            let mut spec = TYPE; // the pseudo-element itself
            for child in named_children(node) {
                spec = spec + specificity_of(child, source);
            }
            spec
        }

        "pseudo_class_selector" => pseudo_class_specificity(node, source),

        // A leaf simple selector reached directly (e.g. a bare `tag_name`
        // already handled above); anything else contributes nothing.
        _ => named_children(node)
            .into_iter()
            .map(|c| specificity_of(c, source))
            .fold(ZERO, |acc, s| acc + s),
    }
}

/// Specificity of a `pseudo_class_selector` node, e.g. `a:hover`, `:is(...)`.
fn pseudo_class_specificity(node: tree_sitter::Node<'_>, source: &[u8]) -> Specificity {
    let mut leading = ZERO; // e.g. the `a` in `a:hover`
    let mut class_name: Option<tree_sitter::Node<'_>> = None;
    let mut arguments: Option<tree_sitter::Node<'_>> = None;

    for child in named_children(node) {
        match child.kind() {
            "class_name" => class_name = Some(child),
            "arguments" => arguments = Some(child),
            _ => leading = leading + specificity_of(child, source),
        }
    }

    let Some(arguments) = arguments else {
        // Plain pseudo-class such as `:hover`.
        return leading + CLASS;
    };

    let name = class_name
        .map(|n| n.utf8_text(source).unwrap_or(""))
        .unwrap_or("");

    match pseudo_function_base(name) {
        // `:where()` — the function and its arguments contribute nothing.
        None => leading,
        Some(base) => {
            let needs_args = matches!(name, "is" | "not" | "has" | "matches");
            if needs_args {
                leading + base + max_argument_specificity(arguments, source)
            } else {
                // `:nth-child()`, `:lang()`, … score as a plain class.
                leading + base
            }
        }
    }
}

/// Most specific of the comma-separated selectors inside `:is()/:not()/…`.
fn max_argument_specificity(arguments: tree_sitter::Node<'_>, source: &[u8]) -> Specificity {
    named_children(arguments)
        .into_iter()
        .map(|s| specificity_of(s, source))
        .fold(ZERO, |acc, s| acc.max(s))
}

/// Text of the rightmost compound of a selector — Biome's grouping key.
///
/// For a combinator (`b a`, `a > b`) it is the trailing operand; for a single
/// compound (`a:hover`, `#b`) it is the whole selector.
fn tail_key(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let tail = match node.kind() {
        "descendant_selector"
        | "child_selector"
        | "sibling_selector"
        | "adjacent_sibling_selector" => named_children(node).into_iter().next_back()?,
        _ => node,
    };
    Some(tail.utf8_text(source).unwrap_or("").trim().to_string())
}

/// Selector node kinds that appear as direct children of a `selectors` list.
fn is_selector_node(kind: &str) -> bool {
    matches!(
        kind,
        "tag_name"
            | "id_selector"
            | "class_selector"
            | "attribute_selector"
            | "universal_selector"
            | "nesting_selector"
            | "pseudo_class_selector"
            | "pseudo_element_selector"
            | "descendant_selector"
            | "child_selector"
            | "sibling_selector"
            | "adjacent_sibling_selector"
            | "namespace_selector"
    )
}

crate::ast_check! { on ["stylesheet"] => |node, source, ctx, diagnostics|
    // Pre-order DFS over every `rule_set` in source order; the visited-tail map
    // is shared across nested rules, exactly like Biome's traversal.
    let mut visited: FxHashMap<String, Specificity> = FxHashMap::default();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        // Push children in reverse so they are popped left-to-right (source order).
        let children = named_children(n);
        for child in children.iter().rev() {
            stack.push(*child);
        }

        if n.kind() != "rule_set" {
            continue;
        }
        let Some(selectors) = named_children(n).into_iter().find(|c| c.kind() == "selectors") else {
            continue;
        };
        for selector in named_children(selectors) {
            if !is_selector_node(selector.kind()) {
                continue;
            }
            let Some(key) = tail_key(selector, source) else { continue; };
            if key.is_empty() {
                continue;
            }
            let spec = specificity_of(selector, source);
            match visited.get(&key) {
                Some(&prev) if prev > spec => {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &selector,
                        super::META.id,
                        format!(
                            "Descending specificity selector found. This selector specificity is {spec}; \
                             an earlier selector with the same tail has higher specificity {prev}."
                        ),
                        Severity::Warning,
                    ));
                }
                Some(_) => {}
                None => {
                    visited.insert(key, spec);
                }
            }
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // ---- Biome invalid fixtures ----

    #[test]
    fn complex_selector_descending() {
        // `b a` (0,0,2) then `a` (0,0,1) — same tail `a`.
        let css = "b a { color: red; }\na { color: red; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn simple_pseudo_selector_descending() {
        // `a:hover #b` (1,1,1) then `a #b` (1,0,1) — same tail `#b`.
        let css = "a:hover #b { color: red; }\na #b { color: red; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn function_pseudo_selector_descending() {
        let css = ":is(#a, a) f { color: red; }\n\
                   :is(a, b, c, d) f { color: red; }\n\
                   :is(#fake#fake#fake#fake#fake#fake, *) g { color: red; }\n\
                   :where(*) g { color: red; }";
        // tail `f`: (1,0,1) then (0,0,2) → 1 flag.
        // tail `g`: (6,0,1) then (0,0,1) → 1 flag.
        assert_eq!(run(css).len(), 2);
    }

    #[test]
    fn nested_descending() {
        // `& > b` (0,0,2) then top-level `b` (0,0,1) — same tail `b`.
        let css = "a {\n  & > b { color: red; }\n}\nb { color: red; }";
        assert_eq!(run(css).len(), 1);
    }

    // ---- Biome valid fixtures ----

    #[test]
    fn ascending_complex_selector_ok() {
        let css = "a { color: red; }\nb a { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ascending_nested_ok() {
        let css = "d { color: red; }\nc {\n  &>d { color: red; }\n}";
        assert!(run(css).is_empty());
    }

    #[test]
    fn compound_pseudo_vs_bare_distinct_tail_ok() {
        // `e:hover` has tail key `e:hover`, bare `e` has tail `e` — different
        // keys, so they are never compared.
        let css = "e:hover { color: red; }\ne { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn is_list_then_id_list_ascending_ok() {
        let css = ":is(a, b, c, d) f { color: red; }\n:is(#a, a) f { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn where_compound_treated_separately_ok() {
        // Different tails (`g` here) means no comparison; `:where()` scores 0.
        let css = ":where(#fake#fake#fake#fake#fake#fake, *) g { color: red; }\n\
                   :where(*) g { color: red; }";
        // tail `g`: first (0,0,1), second (0,0,1) — equal, not descending.
        assert!(run(css).is_empty());
    }

    #[test]
    fn compound_vs_complex_distinct_tail_ok() {
        // `#h h` tail `h`; `:where(#h) :is(h)` tail `:is(h)` — different keys.
        let css = "#h h { color: red; }\n:where(#h) :is(h) { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn media_nesting_equal_specificity_ok() {
        // valid_issue_7085: both `& > p` are (0,0,2) — equal, not descending.
        let css = "div {\n\
                       display: flex;\n\
                       & > p { justify-content: start; }\n\
                       @media (orientation: portrait) {\n\
                         & > p { justify-content: center; }\n\
                       }\n\
                   }";
        assert!(run(css).is_empty());
    }

    // ---- extra guards ----

    #[test]
    fn distinct_tails_never_compared() {
        let css = "b a { color: red; }\nc { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn root_then_html_descending() {
        // From the docblock: `:root input` (0,1,1) then `html input` (0,0,2).
        let css = ":root input { color: red; }\nhtml input { color: red; }";
        assert_eq!(run(css).len(), 1);
    }
}
