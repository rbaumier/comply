//! regex-no-unused-groups — OXC backend.
//! Visits `RegExpLiteral` nodes and flags named capturing groups that are
//! never referenced elsewhere in the file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// Extract named capturing group names from `pattern`.
/// Skips lookbehind constructs `(?<=...)` and `(?<!...)`.
fn extract_named_groups(pattern: &str) -> Vec<(String, usize)> {
    let mut groups = Vec::new();
    let bytes = pattern.as_bytes();
    let needle = b"(?<";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }
        let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
        if backslashes % 2 != 0 {
            i += 1;
            continue;
        }
        let name_start = i + needle.len();
        if name_start < bytes.len() && (bytes[name_start] == b'=' || bytes[name_start] == b'!') {
            i = name_start;
            continue;
        }
        if let Some(rel_end) = pattern[name_start..].find('>') {
            let name = &pattern[name_start..name_start + rel_end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                groups.push((name.to_string(), i));
            }
            i = name_start + rel_end + 1;
        } else {
            break;
        }
    }
    groups
}

/// Checks whether a named group `name` is referenced somewhere in the file,
/// either through direct access (`.groups.name`, `groups["name"]`,
/// `groups['name']`, `$<name>`) or as a key destructured from a `.groups`
/// object (`destructured_keys`, precomputed once per file).
fn is_group_referenced(name: &str, source: &str, destructured_keys: &FxHashSet<String>) -> bool {
    if destructured_keys.contains(name) {
        return true;
    }
    let dot_access = format!(".groups.{name}");
    let bracket_access_dq = format!("groups[\"{name}\"]");
    let bracket_access_sq = format!("groups['{name}']");
    let replacement_ref = format!("$<{name}>");
    crate::oxc_helpers::source_contains(source, &dot_access)
        || crate::oxc_helpers::source_contains(source, &bracket_access_dq)
        || crate::oxc_helpers::source_contains(source, &bracket_access_sq)
        || crate::oxc_helpers::source_contains(source, &replacement_ref)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        let groups = extract_named_groups(pattern);
        if groups.is_empty() {
            return;
        }
        // A `...expr.groups` spread copies every named group out as a property,
        // so no named group in this file can be considered unused.
        if crate::oxc_helpers::file_has_groups_spread(semantic) {
            return;
        }
        let destructured_keys = crate::oxc_helpers::groups_destructure_keys(ctx.source);
        for (name, _offset) in groups {
            if is_group_referenced(&name, ctx.source, &destructured_keys) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Named capturing group `{name}` is never referenced \u{2014} use `.groups.{name}` or convert to `(?:...)`."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unused_named_group() {
        let src = r#"const re = /(?<year>\d{4})-(?<month>\d{2})/;"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_used_named_group_dot() {
        let src = "const re = /(?<year>\\d{4})/;\nconst y = match.groups.year;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_used_named_group_replacement() {
        let src = r#"const re = /(?<day>\d{2})/; str.replace(re, "$<day>");"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_lookbehind() {
        let src = r#"const re = /(?<=foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_negative_lookbehind() {
        let src = r#"const re = /(?<!foo)bar/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_named_group_lookalike_in_string() {
        // OXC only visits RegExpLiteral nodes, so strings are never flagged.
        let src = r#"const x = "grid-[(?<foo>_auto)]";"#;
        assert!(run_on(src).is_empty());
    }

    // --- Regression tests for `.groups` destructuring usage (#3320). ---

    #[test]
    fn allows_groups_destructure_shorthand() {
        let src = "const CODEBLOCK_REGEX = /(?<openingFence>(?<indent>^[ \\t]*))(?<code>[\\s\\S]*?)/gmv;\nconst {code, openingFence, indent} = match.groups ?? {};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_groups_destructure_exec_optional_chain() {
        let src = "const { year } = /(?<year>\\d{4})/.exec(s)?.groups ?? {};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_groups_destructure_renamed_key() {
        let src = "const re = /(?<year>\\d{4})/;\nconst { year: y } = m.groups;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_group_with_no_reference_or_destructure() {
        let src = r#"const re = /(?<unusedGroup>\d+)/;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // --- Regression tests for `...expr.groups` spread usage (#6516). ---

    #[test]
    fn allows_named_group_consumed_via_groups_spread() {
        // unjs/mlly `matchAll` pattern: every named group flows out of the
        // result object via `...match.groups`, never via a direct read.
        let src = "const ESM_STATIC_IMPORT_RE = /import\\s+(?<imports>[\\w]+)\\s+from\\s+[\"'](?<specifier>[^\"']+)[\"']/g;\nfunction matchAll(regex, string, addition) {\n  const matches = [];\n  for (const match of string.matchAll(regex)) {\n    matches.push({ ...addition, ...match.groups, code: match[0] });\n  }\n  return matches;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_named_group_with_optional_chained_groups_spread() {
        let src = "const re = /(?<year>\\d{4})/;\nconst o = { ...m?.groups };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_named_group_when_spread_is_not_dot_groups() {
        // A spread whose final property is not `groups` must not suppress.
        let src = "const re = /(?<year>\\d{4})/;\nconst o = { ...other.fields };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_named_group_when_spread_property_only_prefixes_groups() {
        // `groupsCount` is not `groups`: the match is exact, not a prefix.
        let src = "const re = /(?<year>\\d{4})/;\nconst o = { ...obj.groupsCount };";
        assert_eq!(run_on(src).len(), 1);
    }
}
