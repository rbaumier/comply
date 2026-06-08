//! jsdoc/require-property — `@typedef {Object|object} Foo` must be
//! accompanied by at least one `@property` entry.
//!
//! A typedef that declares its shape as `Object` but never lists any
//! fields has zero documentation value: the reader ends up with the
//! name of a type and nothing else. This rule flags that empty shell.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        let tags = block.tags();
        let Some(typedef) = tags.iter().find(|t| t.name == "typedef") else {
            continue;
        };
        // We only complain when the typedef explicitly types an
        // object. `@typedef {string} Name` is a named primitive —
        // no properties expected.
        if !super::types_object(&typedef.body) {
            continue;
        }
        let has_property = tags
            .iter()
            .any(|t| matches!(t.name.as_str(), "property" | "prop"));
        if !has_property {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: typedef.line + line_offset,
                column: 1,
                rule_id: super::META.id.into(),
                message:
                    "`@typedef` declares an object type but no `@property` entries — document each field."
                        .into(),
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_object_typedef_without_property() {
        let src = r#"
/**
 * @typedef {Object} Point
 */
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_typedef_with_property() {
        let src = r#"
/**
 * @typedef {Object} Point
 * @property {number} x
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_primitive_typedef() {
        let src = r#"
/**
 * @typedef {string} UserId
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_object_alias() {
        let src = r#"
/**
 * @typedef {object} Bare
 */
"#;
        assert_eq!(run(src).len(), 1);
    }
}
