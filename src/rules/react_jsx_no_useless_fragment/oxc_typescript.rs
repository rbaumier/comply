//! react-jsx-no-useless-fragment OxcCheck backend.
//!
//! Flags `<Fragment>` / `<React.Fragment>` wrapping zero or one child.
//! Also handles `<></>` (JSXFragment).
//!
//! An empty fragment (zero meaningful children) inside a test file is the
//! type-test placeholder return — a component written so `expectTypeOf`/
//! `assertType` can probe hook return types in a real component scope renders
//! `<></>` as the minimal valid `JSX.Element`. That is intentional structure,
//! not removable markup, so it is exempt. A single-child fragment stays flagged
//! everywhere (it is always unwrappable), and an empty fragment outside a test
//! file stays flagged (it should be `null`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName, JSXMemberExpressionObject};
use std::sync::Arc;

pub struct Check;

fn is_fragment_tag(name: &JSXElementName) -> bool {
    match name {
        JSXElementName::Identifier(id) => id.name.as_str() == "Fragment",
        JSXElementName::IdentifierReference(id) => id.name.as_str() == "Fragment",
        JSXElementName::MemberExpression(member) => {
            if member.property.name.as_str() != "Fragment" {
                return false;
            }
            matches!(&member.object, JSXMemberExpressionObject::IdentifierReference(obj) if obj.name.as_str() == "React")
        }
        _ => false,
    }
}

fn is_meaningful_child(child: &JSXChild) -> bool {
    match child {
        JSXChild::Text(text) => !text.value.trim().is_empty(),
        _ => true,
    }
}

/// An empty fragment (zero meaningful children) in a test file is the
/// type-test placeholder return — the minimal valid `JSX.Element` a component
/// renders so `expectTypeOf`/`assertType` can probe types in a real component
/// scope. Exempt it. A single-child fragment is unwrappable everywhere, so only
/// the empty form is spared, and only inside a test file.
fn is_type_test_placeholder(meaningful_children: usize, ctx: &CheckCtx) -> bool {
    meaningful_children == 0 && ctx.file.path_segments.in_test_dir
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement, AstType::JSXFragment]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Fragment", "<>"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::JSXOpeningElement(opening) => {
                if !is_fragment_tag(&opening.name) {
                    return;
                }
                // Walk up to the parent JSXElement to count children.
                let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
                    return;
                };
                let AstKind::JSXElement(element) = parent.kind() else {
                    return;
                };
                let meaningful = element
                    .children
                    .iter()
                    .filter(|c| is_meaningful_child(c))
                    .count();
                if is_type_test_placeholder(meaningful, ctx) {
                    return;
                }
                if meaningful <= 1 {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, element.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unnecessary fragment \u{2014} a fragment wrapping zero or one \
                                  child adds no value."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::JSXFragment(frag) => {
                let meaningful = frag
                    .children
                    .iter()
                    .filter(|c| is_meaningful_child(c))
                    .count();
                if is_type_test_placeholder(meaningful, ctx) {
                    return;
                }
                if meaningful <= 1 {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, frag.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unnecessary fragment \u{2014} a fragment wrapping zero or one \
                                  child adds no value."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
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
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;
    use std::path::Path;

    /// Run with a `FileCtx` derived from `path`, so `in_test_dir` reflects the
    /// path (the type-test exemption keys off it).
    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(Path::new(path), src, Language::Tsx, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, &file)
    }

    #[test]
    fn flags_empty_fragment_in_normal_component() {
        // An empty `<></>` in production code should be `null` — still flagged.
        let src = "const X = () => <></>;";
        assert_eq!(run_at(src, "src/App.tsx").len(), 1);
    }

    #[test]
    fn flags_single_child_fragment_in_test_file() {
        // A single-child fragment is unwrappable everywhere, tests included.
        let src = "const X = () => <><div>hi</div></>;";
        assert_eq!(run_at(src, "tests/widget.test.tsx").len(), 1);
    }

    #[test]
    fn allows_type_test_placeholder_fragment_issue1662() {
        // Issue #1662: a type-test component renders `<></>` as the minimal
        // valid JSX.Element so `expectTypeOf` can probe hook return types in a
        // real component scope. The empty fragment is intentional, not markup.
        let src = "const TestComponent = () => {\n  \
            expectTypeOf(useBoundStore((s) => s.count) * 2).toEqualTypeOf<number>()\n  \
            expectTypeOf(useBoundStore((s) => s.inc)()).toEqualTypeOf<void>()\n  \
            return <></>\n};\nexpect(TestComponent).toBeDefined()";
        assert!(run_at(src, "tests/middlewareTypes.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_named_fragment_placeholder_in_test_file() {
        // The named empty `<Fragment></Fragment>` placeholder is exempt too.
        let src = "const X = () => <Fragment></Fragment>;";
        assert!(run_at(src, "src/types.test-d.tsx").is_empty());
    }
}
