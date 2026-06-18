//! no-duplicate-imports OXC backend — flag multiple imports from the same module.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // (module source, is_type_import) -> (first line number, end byte
        // offset of the most recent same-module import). The byte offset lets
        // us inspect the text gap between consecutive same-module imports to
        // honor doc-tooling region markers (see `gap_has_docregion_marker`).
        let mut seen: FxHashMap<(&str, bool), (usize, usize)> = FxHashMap::default();

        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else {
                continue;
            };
            let module = import.source.value.as_str();
            if module.is_empty() {
                continue;
            }
            let is_type = import.import_kind.is_type();
            let key = (module, is_type);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, import.span.start as usize);
            if let Some(&(first_line, prev_end)) = seen.get(&key) {
                // Angular doc examples split same-module imports across
                // `// #docregion` / `// #enddocregion` markers so each named
                // region is a self-contained snippet. When such a marker sits
                // between the previous and current same-module import, the
                // split is intentional doc structure, not a duplicate.
                if gap_has_docregion_marker(ctx.source, prev_end, import.span.start as usize) {
                    seen.insert(key, (first_line, import.span.end as usize));
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate import from `{}` \u{2014} already imported on line {}. Merge into a single statement.",
                        module, first_line
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.insert(key, (line, import.span.end as usize));
            }
        }
        diagnostics
    }
}

/// True when the source text between two consecutive same-module imports
/// (byte range `prev_end..cur_start`) contains an Angular doc-tooling region
/// marker. The gap between top-level `import` statements holds only whitespace
/// and comments, so a plain substring match cannot be fooled by a string
/// literal. Matching `#docregion` also covers `#enddocregion`.
fn gap_has_docregion_marker(source: &str, prev_end: usize, cur_start: usize) -> bool {
    source
        .get(prev_end..cur_start)
        .is_some_and(|gap| gap.contains("#docregion"))
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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_two_imports_from_same_module_issue_1081() {
        // Regression for rbaumier/comply#1081 — two value imports from the
        // same module are flagged once, on the second statement. The
        // duplicate `import-no-duplicates` rule that fired on the same line
        // has been deleted.
        let src = "\
import { foo } from './mod';
import { bar } from './mod';
";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_single_import_per_module() {
        let src = "\
import { foo } from './a';
import { bar } from './b';
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_same_module_imports_split_by_docregion_issue_2296() {
        // Regression for rbaumier/comply#2296 — Angular doc examples split
        // same-module imports across `// #docregion` / `// #enddocregion`
        // markers so each region extracts as a self-contained snippet. The
        // intentional split must not be flagged as a duplicate.
        let src = "\
import {JsonPipe, NgClass} from '@angular/common';
// #docregion import-ng-if
import {NgIf} from '@angular/common';
// #enddocregion import-ng-if
// #docregion import-ng-for
import {NgFor} from '@angular/common';
// #enddocregion import-ng-for
";
        let diags = run(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn still_flags_same_module_imports_without_docregion_issue_2296() {
        // Negative space — two same-module imports with no docregion marker
        // between them are still ordinary duplicates and stay flagged.
        let src = "\
import { foo } from '@angular/common';
import { bar } from '@angular/common';
";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_type_only_import_alongside_value_import() {
        // A `import type` and a value `import` from the same module are
        // distinct statements (different `is_type` key) and not duplicates.
        let src = "\
import { foo } from './mod';
import type { Bar } from './mod';
";
        assert!(run(src).is_empty());
    }
}
