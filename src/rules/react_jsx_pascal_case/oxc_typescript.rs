//! react-jsx-pascal-case oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXElementName, JSXMemberExpression, JSXMemberExpressionObject};
use oxc_span::GetSpan;
use std::sync::Arc;

/// `unstable_`/`experimental_` is an established React convention for
/// experimental public APIs (e.g. `React.unstable_Activity`,
/// `<Checkbox.unstable_BubbleInput />`). The component name after the prefix is
/// still PascalCase, so strip a single leading experimental prefix before the
/// check rather than rejecting the segment for containing `_`.
fn strip_experimental_prefix(segment: &str) -> &str {
    for prefix in ["unstable_", "experimental_"] {
        if let Some(rest) = segment.strip_prefix(prefix) {
            return rest;
        }
    }
    segment
}

fn is_pascal_case(name: &str) -> bool {
    for raw_segment in name.split('.') {
        let segment = strip_experimental_prefix(raw_segment);
        if segment.is_empty() {
            return false;
        }
        let first = segment.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return false;
        }
        if segment.contains('_') || segment.contains('-') {
            return false;
        }
    }
    true
}

/// Component names sometimes carry an underscore-delimited namespace/visibility
/// marker after a PascalCase base — the shadcn-ui re-export convention
/// (`Input_Shadcn_`, `SelectItem_Shadcn_`) disambiguates a wrapped component
/// from an identically-named local one. The base is PascalCase; the underscore
/// suffix is the discriminator. Accept the whole name when the segment before
/// the first `_` is genuine mixed-case PascalCase. Requiring a lowercase letter
/// in the base keeps `SCREAMING_SNAKE_CASE` names (`MY_COMPONENT`) flagged while
/// admitting real component bases (`Input`, `SelectItem`).
fn has_pascal_case_base_with_underscore_suffix(name: &str) -> bool {
    let Some((base, _suffix)) = name.split_once('_') else {
        return false;
    };
    is_pascal_case(base) && base.chars().any(|c| c.is_ascii_lowercase())
}

fn is_intrinsic(name: &str) -> bool {
    let first = name.chars().next().unwrap_or('a');
    first.is_ascii_lowercase()
}

/// The leftmost identifier of a JSX member expression (`Table` in `Table.td`,
/// `Foo` in `Foo.Bar.Baz`). A `this` root (`<this.Orange />`) has no identifier
/// name and yields `None`.
fn member_root_name<'a>(member: &'a JSXMemberExpression<'a>) -> Option<&'a str> {
    match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => Some(id.name.as_str()),
        JSXMemberExpressionObject::MemberExpression(inner) => member_root_name(inner),
        JSXMemberExpressionObject::ThisExpression(_) => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // For a member expression (`Table.td`, `Form.Item`), the PascalCase
        // requirement applies only to the leftmost/root identifier — the
        // property accessor is the compound-component sub-name and is exempt. A
        // lowercase root (`table.Foo`) is a real violation, not an intrinsic
        // host element, so the member-expression root skips the intrinsic
        // exemption. Plain identifiers and namespaced names keep the full check.
        let tag = match &opening.name {
            JSXElementName::Identifier(id) => {
                let tag = id.name.as_str();
                if is_intrinsic(tag) {
                    return;
                }
                tag.to_string()
            }
            JSXElementName::IdentifierReference(id) => {
                let tag = id.name.as_str();
                if is_intrinsic(tag) {
                    return;
                }
                tag.to_string()
            }
            JSXElementName::MemberExpression(member) => {
                // `Namespace.htmlElement` (e.g. `Primitive.div`, `Table.td`) is a
                // valid compound-component / styled-components namespace and is
                // never flagged on its sub-name.
                if is_intrinsic(member.property.name.as_str()) {
                    return;
                }
                // A `this`-rooted member (`<this.Orange />`) has no component
                // identifier to validate.
                let Some(root) = member_root_name(member) else {
                    return;
                };
                root.to_string()
            }
            JSXElementName::NamespacedName(ns) => {
                let tag = format!("{}:{}", ns.namespace.name, ns.name.name);
                if is_intrinsic(&tag) {
                    return;
                }
                tag
            }
            _ => return,
        };

        if !is_pascal_case(&tag) && !has_pascal_case_base_with_underscore_suffix(&tag) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.name.span().start as usize);

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Component `{tag}` is not PascalCase — rename to PascalCase."
                ),
                severity: Severity::Warning,
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

    // Issue #1986: `Namespace.htmlElement` member-expression components (Radix-UI /
    // styled-components) whose final segment is a lowercase intrinsic HTML element
    // are valid and must not be flagged.
    #[test]
    fn allows_namespaced_intrinsic_div() {
        assert!(run("const x = <Primitive.div />;").is_empty());
    }

    #[test]
    fn allows_namespaced_intrinsic_button() {
        assert!(run("const x = <Primitive.button />;").is_empty());
    }

    #[test]
    fn allows_namespaced_intrinsic_span() {
        assert!(run("const x = <Primitive.span />;").is_empty());
    }

    #[test]
    fn allows_namespaced_pascal() {
        // `Foo.Bar` (PascalCase final segment) was already valid — must not regress.
        assert!(run("const x = <Foo.Bar />;").is_empty());
    }

    #[test]
    fn flags_non_pascal_case_component() {
        // A genuinely bad component name (not a member expression) still fires.
        assert_eq!(run("const x = <MY_COMPONENT />;").len(), 1);
    }

    #[test]
    fn allows_plain_intrinsic() {
        assert!(run("const x = <div>hello</div>;").is_empty());
    }

    // Issue #1987: `unstable_`/`experimental_` is an established React convention
    // for experimental public APIs (e.g. `<Checkbox.unstable_BubbleInput />`). The
    // underlying component name stays PascalCase, so these must not be flagged.
    #[test]
    fn allows_unstable_member_bubble_input() {
        assert!(run("const x = <Checkbox.unstable_BubbleInput />;").is_empty());
    }

    #[test]
    fn allows_unstable_member_provider() {
        assert!(run("const x = <Checkbox.unstable_Provider />;").is_empty());
    }

    #[test]
    fn allows_unstable_member_trigger() {
        assert!(run("const x = <Checkbox.unstable_Trigger />;").is_empty());
    }

    #[test]
    fn is_pascal_case_accepts_experimental_prefixes() {
        // Direct coverage of the prefix-stripping in `is_pascal_case`, independent
        // of the dispatch guards: the experimental prefix is stripped, then the
        // remainder is checked for PascalCase.
        assert!(is_pascal_case("Checkbox.unstable_BubbleInput"));
        assert!(is_pascal_case("Checkbox.experimental_Trigger"));
        // A non-experimental underscore in an uppercase segment still fails.
        assert!(!is_pascal_case("Foo.Bad_Name"));
        // The prefix must be a literal `unstable_`/`experimental_`; an uppercase
        // look-alike is not the convention and still fails.
        assert!(!is_pascal_case("Unstable_Thing"));
    }

    // Issue #1370: compound components accessed via dot notation use lowercase
    // sub-names that mirror native HTML elements (`Table.td`, `Table.th`,
    // `Table.tr`). Only the root (`Table`) must be PascalCase; the sub-name is
    // exempt.
    #[test]
    fn allows_compound_component_td() {
        assert!(run("const x = <Table.td>x</Table.td>;").is_empty());
    }

    #[test]
    fn allows_compound_component_th() {
        assert!(run("const x = <Table.th>x</Table.th>;").is_empty());
    }

    #[test]
    fn allows_compound_component_tr() {
        assert!(run("const x = <Table.tr>x</Table.tr>;").is_empty());
    }

    #[test]
    fn allows_compound_component_pascal_subname() {
        // `Form.Item` — non-intrinsic PascalCase sub-name, PascalCase root.
        assert!(run("const x = <Form.Item>x</Form.Item>;").is_empty());
    }

    // Positive space: a member expression whose ROOT is not PascalCase still
    // fires on the root, even when the sub-name is fine.
    #[test]
    fn flags_lowercase_root_member_expression() {
        assert_eq!(run("const x = <table.Foo>x</table.Foo>;").len(), 1);
    }

    #[test]
    fn flags_camel_root_member_expression() {
        assert_eq!(run("const x = <myObj.Bar>x</myObj.Bar>;").len(), 1);
    }

    // Issue #1352: the shadcn-ui re-export convention appends an
    // underscore-delimited namespace marker (`_Shadcn_`) to a PascalCase base to
    // disambiguate a wrapped component from an identically-named local one. The
    // base is PascalCase, so these must not be flagged.
    #[test]
    fn allows_underscore_suffix_self_closing() {
        assert!(run("const x = <Input_Shadcn_ className=\"y\" />;").is_empty());
    }

    #[test]
    fn allows_underscore_suffix_with_children() {
        assert!(run("const x = <SelectItem_Shadcn_>x</SelectItem_Shadcn_>;").is_empty());
    }

    // Negative space: a name whose base (before the first `_`) is not valid
    // PascalCase is not a namespace-marker convention and still fires. A leading
    // underscore leaves an empty base (`_DataTable`), so the relaxation does not
    // apply. (`is_intrinsic` already exempts lowercase-first names like
    // `foo_Bar`/`myComponent` as host elements before this point — the helper's
    // lowercase-base rejection is covered directly in the unit test below.)
    #[test]
    fn flags_leading_underscore_empty_base() {
        assert_eq!(run("const x = <_DataTable />;").len(), 1);
    }

    #[test]
    fn has_pascal_case_base_with_underscore_suffix_decisions() {
        // PascalCase base + underscore marker → accepted.
        assert!(has_pascal_case_base_with_underscore_suffix("Input_Shadcn_"));
        assert!(has_pascal_case_base_with_underscore_suffix("SelectItem_Shadcn_"));
        assert!(has_pascal_case_base_with_underscore_suffix("Alert_Shadcn_"));
        // Lowercase base → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("foo_Bar"));
        // SCREAMING_SNAKE_CASE base (no lowercase) → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("MY_COMPONENT"));
        // Leading underscore (empty base) → rejected.
        assert!(!has_pascal_case_base_with_underscore_suffix("_DataTable"));
        // No underscore → not this convention (handled by `is_pascal_case`).
        assert!(!has_pascal_case_base_with_underscore_suffix("Input"));
    }
}
