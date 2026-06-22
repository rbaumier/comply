use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Extract the quoted path value from a `/// <reference path="..." />` line.
/// Returns `None` if no `path=` attribute with a quoted value is present.
fn reference_path_value(line: &str) -> Option<&str> {
    let after = line.split_once("path=")?.1;
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    after[1..].split(quote).next()
}

/// Whether a referenced path targets a TypeScript declaration file.
fn targets_declaration_file(path: &str) -> bool {
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["///"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for line_text in ctx.source.lines() {
            let trimmed = line_text.trim();
            if !trimmed.starts_with("/// <reference") && !trimmed.starts_with("///<reference") {
                continue;
            }
            // Only `path=` references import a file and have a clean ES `import`
            // replacement. `types=` (ambient `@types` / global augmentations) and
            // `lib=` (built-in libs) pull in declarations with no ESM equivalent.
            if !trimmed.contains("path=") {
                continue;
            }
            // A `path=` reference to a declaration file (`.d.ts`/`.d.mts`/`.d.cts`)
            // pulls in type declarations — the canonical legitimate use, and the
            // only way to include shared ambient `declare module`/`declare global`
            // stubs (which cannot be expressed as ES imports). Flag only `path=`
            // references to `.ts` source modules, which do have an import equivalent.
            if reference_path_value(trimmed).is_some_and(targets_declaration_file) {
                continue;
            }
            let byte_offset = line_text.as_ptr() as usize - ctx.source.as_ptr() as usize;
            let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Triple-slash `path` reference directive is legacy — \
                          use ES `import` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #5697: a `path=` reference to a shared `.d.ts` declaration
    // file pulls in ambient `declare module` stubs for untyped packages — the
    // canonical legitimate use, with no ES-import equivalent. Must not flag.
    #[test]
    fn declaration_file_path_reference_is_not_flagged() {
        assert!(run("/// <reference path=\"../../../__typings__/index.d.ts\"/>\n").is_empty());
    }

    #[test]
    fn declaration_file_with_single_quotes_is_not_flagged() {
        assert!(run("/// <reference path='./local.d.ts' />\n").is_empty());
    }

    #[test]
    fn declaration_file_mts_cts_are_not_flagged() {
        assert!(run("/// <reference path=\"./types.d.mts\" />\n").is_empty());
        assert!(run("/// <reference path=\"./types.d.cts\" />\n").is_empty());
    }

    // A `path=` reference to a `.ts` source module still flags — it has a clean
    // ES `import` replacement, which is the rule's actual target.
    #[test]
    fn source_module_path_reference_still_flags() {
        let diags = run("/// <reference path=\"./helper.ts\" />\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    // `types=` and `lib=` references have no ESM equivalent and are never flagged.
    #[test]
    fn types_and_lib_references_are_not_flagged() {
        assert!(run("/// <reference types=\"vitest\" />\n").is_empty());
        assert!(run("/// <reference lib=\"dom\" />\n").is_empty());
    }
}
