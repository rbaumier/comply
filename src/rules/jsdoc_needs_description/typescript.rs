//! jsdoc-needs-description backend — flag JSDoc blocks that have tags but no description.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !text.starts_with("/**") {
        return;
    }

    let mut tags: Vec<&str> = Vec::new();
    let mut has_description = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let content = trimmed
            .trim_start_matches("/**")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();

        if content.is_empty() || content == "/" {
            continue;
        }

        if content.starts_with('@') {
            if let Some(tag) = content
                .trim_start_matches('@')
                .split_whitespace()
                .next()
            {
                tags.push(tag);
            }
        } else {
            has_description = true;
        }
    }

    if !tags.is_empty()
        && !has_description
        && !tags
            .iter()
            .all(|tag| is_type_only_tag(tag) || is_pragma_tag(tag))
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "jsdoc-needs-description".into(),
            message: "JSDoc block contains only tags — add a prose description explaining what this does and why.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_type_only_tag(tag: &str) -> bool {
    matches!(
        tag,
        "type"
            | "param"
            | "arg"
            | "argument"
            | "returns"
            | "return"
            | "template"
            | "typedef"
            | "callback"
            | "property"
            | "prop"
            | "this"
            | "implements"
            | "extends"
            | "satisfies"
    )
}

/// JSX compiler pragma directives (Babel/TypeScript) carried in JSDoc syntax.
/// The whole comment is the directive consumed by the compiler — there is no
/// prose description to add.
fn is_pragma_tag(tag: &str) -> bool {
    matches!(tag, "jsx" | "jsxImportSource" | "jsxRuntime" | "jsxFrag")
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_param_return_type_signature() {
        let source = r#"
/**
 * @param x - the input
 * @returns the output
 */
function foo(x: number): number { return x; }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_type_only_jsdoc_annotation() {
        let source = r#"
/** @type {import('@sveltejs/kit').Config} */
const config = {};
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_single_line_tag_only() {
        let source = "/** @deprecated */\nfunction old() {}";
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_jsdoc_with_description() {
        let source = r#"
/**
 * Computes the square of a number.
 * @param x - the input
 * @returns the squared value
 */
function square(x: number): number { return x * x; }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsdoc_with_description_only() {
        let source = r#"
/**
 * This function does something important.
 */
function important() {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_empty_jsdoc() {
        let source = r#"
/**
 */
function foo() {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsx_pragma() {
        let source = "/** @jsx jsx */\nimport { jsx } from '@emotion/react'";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsx_import_source_pragma() {
        let source = "/** @jsxImportSource @emotion/react */\nexport const x = 1;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsx_runtime_pragma() {
        let source = "/** @jsxRuntime classic */\nexport const x = 1;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_jsx_frag_pragma() {
        let source = "/** @jsxFrag jsxFrag */\nexport const x = 1;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_non_pragma_non_type_tag() {
        let source = "/** @deprecated */\nfunction old() {}";
        let d = run_on(source);
        assert_eq!(d.len(), 1, "@deprecated is neither pragma nor type-only");
    }

    #[test]
    fn allows_pragma_with_description() {
        let source = r#"
/**
 * Configures the JSX runtime for this module.
 * @jsxImportSource @emotion/react
 */
export const x = 1;
"#;
        assert!(run_on(source).is_empty());
    }
}
