//! redundant-type-aliases oxc backend — flag `type X = Y` where Y is a single type.
//!
//! Skip when the alias is `export`ed (public API surface) or carries a leading
//! `/** … */` JSDoc block (the comment proves documentation value).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_ast::CommentKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else { return };

        // Only flag if the alias has no type parameters (not generic).
        if alias.type_parameters.is_some() {
            return;
        }

        // Skip exported aliases — the public surface is the alias name, not its expansion.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(
            parent.kind(),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
        ) {
            return;
        }

        // Skip aliases preceded by a JSDoc (`/** … */`) block — the comment is documentation.
        if has_leading_jsdoc(ctx.source, semantic, alias.span.start as usize) {
            return;
        }

        // Only flag if the RHS is a single type identifier or predefined type
        // (plain name like `Foo` or primitive like `string`).
        let is_simple = matches!(
            &alias.type_annotation,
            TSType::TSTypeReference(ref_ty)
                if ref_ty.type_arguments.is_none()
                    && matches!(
                        &ref_ty.type_name,
                        oxc_ast::ast::TSTypeName::IdentifierReference(_)
                    )
        ) || matches!(
            &alias.type_annotation,
            TSType::TSStringKeyword(_)
                | TSType::TSNumberKeyword(_)
                | TSType::TSBooleanKeyword(_)
                | TSType::TSAnyKeyword(_)
                | TSType::TSNeverKeyword(_)
                | TSType::TSNullKeyword(_)
                | TSType::TSUndefinedKeyword(_)
                | TSType::TSVoidKeyword(_)
                | TSType::TSBigIntKeyword(_)
                | TSType::TSSymbolKeyword(_)
                | TSType::TSObjectKeyword(_)
                | TSType::TSUnknownKeyword(_)
        );

        if !is_simple {
            return;
        }

        // Skip if the alias name appears ≥ 3 times in the file (declaration + 2+ uses).
        // An alias reused this many times is a semantic anchor; flagging it contradicts `use-type-alias`.
        let alias_name = alias.id.name.as_str();
        if count_identifier_occurrences(ctx.source, alias_name) >= 3 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Type alias is just renaming \u{2014} use the original type directly or add structure.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Counts non-overlapping whole-word occurrences of `name` in `source`.
fn count_identifier_occurrences(source: &str, name: &str) -> usize {
    let src = source.as_bytes();
    let nm = name.as_bytes();
    let nm_len = nm.len();
    if nm_len == 0 {
        return 0;
    }
    let mut count = 0;
    let mut pos = 0;
    while pos + nm_len <= src.len() {
        if src[pos..pos + nm_len] == *nm {
            let before_ok = pos == 0 || {
                let b = src[pos - 1];
                !b.is_ascii_alphanumeric() && b != b'_'
            };
            let after_ok = pos + nm_len == src.len() || {
                let b = src[pos + nm_len];
                !b.is_ascii_alphanumeric() && b != b'_'
            };
            if before_ok && after_ok {
                count += 1;
            }
        }
        pos += 1;
    }
    count
}

/// Returns true when the byte range immediately before `decl_start` ends with
/// a `/** … */` JSDoc block (ignoring whitespace, including an optional
/// `export` keyword between the comment and the declaration).
fn has_leading_jsdoc(
    source: &str,
    semantic: &oxc_semantic::Semantic<'_>,
    decl_start: usize,
) -> bool {
    for comment in semantic.comments() {
        // Line comments (`//`) are never JSDoc.
        if comment.kind == CommentKind::Line {
            continue;
        }
        let comment_end = comment.span.end as usize;
        if comment_end > decl_start {
            continue;
        }
        let between = match source.get(comment_end..decl_start) {
            Some(s) => s,
            None => continue,
        };
        // Allow whitespace and a leading `export` between the comment and the alias.
        let trimmed = between.trim_start().trim_end();
        if !trimmed.is_empty() && trimmed != "export" {
            continue;
        }
        let comment_start = comment.span.start as usize;
        // OXC spans for block comments include the `/*` delimiter, so
        // `source[comment_start..comment_end]` starts with `/**` for JSDoc.
        let Some(raw) = source.get(comment_start..comment_end) else {
            continue;
        };
        if raw.starts_with("/**") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn flags_simple_rename() {
        assert_eq!(run_oxc_ts("type UserID = string;", &Check).len(), 1);
    }

    #[test]
    fn flags_identifier_rename() {
        assert_eq!(run_oxc_ts("type Alias = OriginalType;", &Check).len(), 1);
    }

    #[test]
    fn allows_union_type() {
        assert!(run_oxc_ts("type X = string | number;", &Check).is_empty());
    }

    #[test]
    fn skips_exported_alias() {
        // Public surface — exported alias name is the API.
        assert!(run_oxc_ts("export type UserID = string;", &Check).is_empty());
    }

    #[test]
    fn skips_alias_with_leading_jsdoc() {
        // Documented domain alias — the comment carries semantic value.
        let src = "/** Stable id for a user. */\ntype UserID = string;";
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn skips_exported_alias_with_jsdoc_regression_145() {
        // Regression for https://github.com/rbaumier/comply/issues/145 — `export`
        // alone suppresses, JSDoc alone suppresses, both together suppress.
        let src = "/** Shape produced by every multi-select filter. */\nexport type ListFilterValues = ReadonlyStrings;";
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn still_flags_non_jsdoc_line_comment() {
        // `//` comments are not JSDoc — should not suppress.
        let src = "// just a note\ntype Alias = Original;";
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn still_flags_block_comment_non_jsdoc() {
        // `/* */` (single star) is not JSDoc — should not suppress.
        let src = "/* not jsdoc */\ntype Alias = Original;";
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn skips_reused_alias_regression_371() {
        // Non-exported, no JSDoc, but used ≥ 3 times — semantic anchor used across the file.
        // Flagging this would contradict `use-type-alias`.
        let src = r#"
type ListFilterValues = string;
function applyFilter(v: ListFilterValues) {}
function validateFilter(v: ListFilterValues) {}
function resetFilter(v: ListFilterValues) {}
"#;
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn still_flags_alias_used_only_once() {
        // Declaration + 1 use = 2 occurrences — still a redundant rename.
        let src = "type Alias = string;\nfunction foo(v: Alias) {}";
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn allows_intersection_type() {
        assert!(run_on("type X = A & B;").is_empty());
    }


    #[test]
    fn allows_generic_type() {
        assert!(run_on("type X = Array<string>;").is_empty());
    }


    #[test]
    fn allows_object_type() {
        assert!(run_on("type X = { name: string };").is_empty());
    }
}
