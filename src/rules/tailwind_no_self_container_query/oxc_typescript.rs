use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::tailwind_class_strings::{ClassString, element_class_strings};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@container"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let Some(variant) = self_query(&element_class_strings(opening, semantic)) else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{variant}:` sits on the element that declares `@container`, so it never matches. Move `@container` to a parent."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// First unnamed container variant sharing a branch with `@container`.
fn self_query<'a>(strings: &[ClassString<'a>]) -> Option<&'a str> {
    strings
        .iter()
        .filter(|string| {
            string.classes.iter().any(|class| {
                class.utility == "@container" || class.utility.starts_with("@container/")
            })
        })
        .find_map(|container| {
            strings
                .iter()
                .filter(|string| string.branch == container.branch)
                .flat_map(|string| &string.classes)
                .flat_map(|class| &class.variants)
                .copied()
                .find(|variant| variant.starts_with('@') && !variant.contains('/'))
        })
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
    ) -> Vec<Diagnostic> {
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
    fn flags_container_and_query_in_one_literal() {
        let diagnostics = run(r#"const a = <div className="@container @md:flex" />;"#);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("`@md:`"));
    }

    #[test]
    fn flags_unconditional_helper_arguments() {
        let diagnostics = run(r#"const a = <div className={cn("@container", "@lg:grid")} />;"#);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("`@lg:`"));
    }

    #[test]
    fn allows_exclusive_ternary_arms() {
        assert!(
            run(r#"const a = <div className={wide ? "@container" : "@md:flex"} />;"#).is_empty()
        );
    }

    #[test]
    fn flags_same_literal_inside_arm() {
        let src = r#"const a = <div className={wide ? "@container @md:flex" : "p-4"} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_separate_literals_in_same_arm() {
        let src = r#"const a = <div className={wide ? cn("@container", "@md:flex") : "p-4"} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_named_query_variant() {
        assert!(run(r#"const a = <div className="@container/main @3xl/side:flex" />;"#).is_empty());
    }

    #[test]
    fn allows_container_normal() {
        assert!(run(r#"const a = <div className="@container-normal @md:flex" />;"#).is_empty());
    }

    #[test]
    fn flags_range_and_arbitrary_query_variants() {
        let src = r#"const a = <>
            <div className="@container @max-md:flex" />
            <div className="@container @min-[400px]:grid" />
            <div className="@container @[17.5rem]:p-4" />
        </>;"#;
        assert_eq!(run(src).len(), 3);
    }
}
