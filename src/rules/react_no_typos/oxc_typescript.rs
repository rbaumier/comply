//! react-no-typos OxcCheck backend.
//!
//! Flags probable typos in React static properties and lifecycle methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, PropertyKey};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// React component base classes. A class is only checked for lifecycle/static
/// typos when it extends one of these (directly or via a namespace alias such
/// as `React.Component`).
const REACT_BASES: &[&str] = &["Component", "PureComponent"];

/// True when the member's enclosing class extends a React component base.
///
/// Walks ancestors to the nearest `Class`, then inspects its `super_class`:
/// `Component`/`PureComponent` directly, or `<Ident>.Component` /
/// `<Ident>.PureComponent` (e.g. `React.Component`). Anything else — no
/// superclass, or a non-React base — is not a React component.
fn is_in_react_component<'a>(
    node: &oxc_semantic::AstNode<'a>,
    ctx: &CheckCtx,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        let AstKind::Class(class) = ancestor.kind() else {
            continue;
        };
        let Some(super_class) = &class.super_class else {
            return false;
        };
        let start = super_class.span().start as usize;
        let end = super_class.span().end as usize;
        if end > ctx.source.len() {
            return false;
        }
        // Last segment after `.` (e.g. `React.Component` -> `Component`).
        let base = ctx.source[start..end]
            .rsplit('.')
            .next()
            .unwrap_or(&ctx.source[start..end]);
        return REACT_BASES.contains(&base);
    }
    false
}

/// Plain member name of a class element, or `None` for computed / non-identifier
/// keys and static blocks.
fn class_element_name<'a>(element: &'a ClassElement<'a>) -> Option<&'a str> {
    let key = match element {
        ClassElement::MethodDefinition(method) => &method.key,
        ClassElement::PropertyDefinition(prop) => &prop.key,
        ClassElement::AccessorProperty(accessor) => &accessor.key,
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => return None,
    };
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// Names of every member of the node's nearest enclosing class.
///
/// A correctly-spelled lifecycle name present here proves the author knows the
/// spelling, so a same-class member that merely resembles it is not a typo.
fn enclosing_class_member_names<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> FxHashSet<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        let AstKind::Class(class) = ancestor.kind() else {
            continue;
        };
        return class.body.body.iter().filter_map(class_element_name).collect();
    }
    FxHashSet::default()
}

/// Correct React lifecycle methods and static properties.
const KNOWN_NAMES: &[&str] = &[
    "getDerivedStateFromProps",
    "componentWillMount",
    "UNSAFE_componentWillMount",
    "componentDidMount",
    "componentWillReceiveProps",
    "UNSAFE_componentWillReceiveProps",
    "shouldComponentUpdate",
    "componentWillUpdate",
    "UNSAFE_componentWillUpdate",
    "getSnapshotBeforeUpdate",
    "componentDidUpdate",
    "componentDidCatch",
    "componentWillUnmount",
    "render",
    "defaultProps",
    "displayName",
    "propTypes",
    "contextTypes",
    "childContextTypes",
    "contextType",
];

/// Simple Levenshtein distance (bounded).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for (j, item) in prev.iter_mut().enumerate().take(n + 1) {
        *item = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Returns the lifecycle/static name `name` most likely misspells, or `None`.
///
/// A correction already present in `sibling_members` is skipped: if the exact
/// lifecycle name exists in the same class, `name` is a distinct intentional
/// member, not a typo of it.
fn is_probable_typo(name: &str, sibling_members: &FxHashSet<&str>) -> Option<&'static str> {
    for &known in KNOWN_NAMES {
        if name == known {
            return None;
        }
    }
    for &known in KNOWN_NAMES {
        if sibling_members.contains(known) {
            continue;
        }
        let dist = edit_distance(name, known);
        if known.len() > 5 && dist > 0 && dist <= 2 {
            return Some(known);
        }
    }
    None
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition, AstType::PropertyDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Lifecycle/static typo names only make sense on React component
        // classes; skip members of any other class to avoid false positives
        // on names that merely resemble React identifiers.
        if !is_in_react_component(node, ctx, semantic) {
            return;
        }

        let name = match node.kind() {
            AstKind::MethodDefinition(method) => {
                if let PropertyKey::StaticIdentifier(ident) = &method.key {
                    Some((ident.name.as_str(), ident.span))
                } else {
                    None
                }
            }
            AstKind::PropertyDefinition(prop) => {
                if let PropertyKey::StaticIdentifier(ident) = &prop.key {
                    Some((ident.name.as_str(), ident.span))
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some((name, span)) = name else { return };

        let sibling_members = enclosing_class_member_names(node, semantic);
        if let Some(correction) = is_probable_typo(name, &sibling_members) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "react-no-typos".into(),
                message: format!("`{name}` is a probable typo — did you mean `{correction}`?"),
                severity: Severity::Error,
                span: None,
            });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn ignores_sender_field_in_plain_class() {
        // azure-sdk-for-js FP: `_sender` is edit-distance 2 from `render`,
        // but ServiceBusSenderImpl is not a React component.
        let src = "class ServiceBusSenderImpl {\n  private _sender: ServiceBusSender;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_content_type_field_in_plain_class() {
        // azure-sdk-for-js FP: `contentType` is close to `contextTypes`,
        // but this HTTP message class is not a React component.
        let src = "class HttpMessage {\n  public contentType?: string;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_typo_in_class_with_no_superclass() {
        let src = "class Foo {\n  componentDidMouhnt() {}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_typo_in_class_with_non_react_superclass() {
        let src = "class Foo extends Base {\n  componentDidMouhnt() {}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lifecycle_typo_in_react_namespace_component() {
        let src = "class Comp extends React.Component {\n  componentDidMouhnt() {}\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("componentDidMount"));
    }

    #[test]
    fn flags_lifecycle_typo_in_bare_component() {
        let src = "class Comp extends Component {\n  shouldComponentUpdat() {}\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shouldComponentUpdate"));
    }

    #[test]
    fn flags_static_prop_typo_in_pure_component() {
        let src = "class Comp extends React.PureComponent {\n  static defautProps = {};\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defaultProps"));
    }

    #[test]
    fn allows_correct_lifecycle_in_react_component() {
        let src = "class Comp extends React.Component {\n  componentDidMount() {}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_member_when_exact_lifecycle_exists_in_same_class() {
        // formik FP: `renders` is edit-distance 1 from `render`, but `render`
        // already exists in the same class — the author knows the spelling.
        let src = "class Input extends React.Component {\n  renders = 0;\n  render() { return null; }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_member_when_exact_lifecycle_absent_from_class() {
        // No `render` method present, so `renders` is a genuine probable typo.
        let src = "class Input extends React.Component {\n  renders = 0;\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("render"));
    }
}
