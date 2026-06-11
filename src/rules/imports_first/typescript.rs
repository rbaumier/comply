//! AST-based check for `imports-first`.
//!
//! Walks the top-level `program` node once. Named children that are
//! `import_statement` nodes are the "imports block"; anything else is a
//! real statement that flips `saw_non_import` to true. After the flip,
//! any later `import_statement` is flagged.
//!
//! `export { x } from "./y"` is an `export_statement` with a `source`
//! child — by convention these sit in the import block too, so they're
//! treated as imports (not as a non-import statement that terminates the
//! block). They are *not* flagged if they appear after code, because the
//! rule is specifically about `import` statements.
//!
//! Dynamic `import("./x")` is a `call_expression`, not an
//! `import_statement`, so it never triggers.
//!
//! Comments are unnamed children in tree-sitter-typescript and are
//! skipped by `named_children`; blank lines are not nodes at all.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut saw_non_import = false;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                if saw_non_import {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &child,
                        "imports-first",
                        "Import statement after non-import code — move to the top of the file.".into(),
                        Severity::Warning,
                    ));
                }
            }
            // `export ... from "..."` re-exports conventionally live in
            // the import block. Don't flip the flag on them.
            "export_statement" if child.child_by_field_name("source").is_some() => {}
            // Comments, hashbangs, and string-literal directive prologues
            // are not "real code" and can appear before or between imports.
            "comment" | "hash_bang_line" => {}
            "expression_statement"
                if is_directive_prologue(&child, ctx.source.as_bytes()) => {}
            // Test-framework configuration calls placed before imports are a
            // widespread convention (`jest.setTimeout`, `vi.setConfig`,
            // `jasmine.DEFAULT_TIMEOUT_INTERVAL = N`). They have no import
            // side effects so they do not break the imports-first invariant.
            "expression_statement"
                if is_test_framework_config(&child, ctx.source.as_bytes()) => {}
            _ => {
                saw_non_import = true;
            }
        }
    }
}

/// A directive prologue is an `expression_statement` whose only child is
/// a `string` literal (e.g. `"use strict";`, `"use client";`).
fn is_directive_prologue(node: &tree_sitter::Node<'_>, _source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let mut named = node.named_children(&mut cursor);
    let Some(first) = named.next() else {
        return false;
    };
    if named.next().is_some() {
        return false;
    }
    first.kind() == "string"
}

/// Test-framework configuration calls that are conventionally placed before
/// imports:
/// - `jest.setTimeout(N)` — sets the default test timeout for the file
/// - `vi.setConfig({ testTimeout: N })` — Vitest equivalent
/// - `jasmine.DEFAULT_TIMEOUT_INTERVAL = N` — Jasmine equivalent (assignment)
///
/// These are zero-import-side-effect statements and must not flip `saw_non_import`.
fn is_test_framework_config(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let mut named = node.named_children(&mut cursor);
    let Some(inner) = named.next() else {
        return false;
    };
    if named.next().is_some() {
        return false;
    }

    // `jest.setTimeout(N)` / `vi.setConfig(...)` — call_expression
    if inner.kind() == "call_expression"
        && let Some(callee) = inner.child_by_field_name("function")
        && callee.kind() == "member_expression"
        && let Some(obj) = callee.child_by_field_name("object")
        && let Some(prop) = callee.child_by_field_name("property")
    {
        let obj_name = obj.utf8_text(source).unwrap_or("");
        let prop_name = prop.utf8_text(source).unwrap_or("");
        return matches!(
            (obj_name, prop_name),
            ("jest", "setTimeout") | ("vi", "setConfig")
        );
    }

    // `jasmine.DEFAULT_TIMEOUT_INTERVAL = N` — assignment_expression
    if inner.kind() == "assignment_expression"
        && let Some(left) = inner.child_by_field_name("left")
        && left.kind() == "member_expression"
        && let Some(obj) = left.child_by_field_name("object")
        && let Some(prop) = left.child_by_field_name("property")
    {
        let obj_name = obj.utf8_text(source).unwrap_or("");
        let prop_name = prop.utf8_text(source).unwrap_or("");
        return obj_name == "jasmine" && prop_name == "DEFAULT_TIMEOUT_INTERVAL";
    }

    false
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
    
    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_import_after_code() {
        let src = r#"const x = 1;
import { a } from 'a';
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_imports_at_top() {
        let src = r#"import { a } from 'a';
import { b } from 'b';
const x = 1;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_directive_before_imports() {
        let src = r#"'use strict';
import { a } from 'a';
const x = 1;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiline_import_block_at_top() {
        let src = r#"
import {
  A,
  B,
  C,
} from "x";
import Y from "y";
const z = 1;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_import_after_non_import() {
        let src = r#"
import A from "x";
const z = 1;
import B from "y";
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_dynamic_import_expression() {
        let src = r#"
const z = 1;
const mod = await import("./y");
const w = 2;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_blank_lines_between_imports() {
        let src = r#"
import A from "x";

import B from "y";

import C from "z";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comments_between_imports() {
        let src = r#"
import A from "x";
// comment
import B from "y";
"#;
        assert!(run_on(src).is_empty());
    }

    // Regression test for https://github.com/rbaumier/comply/issues/987
    // `jest.setTimeout` before imports is a widespread convention and must not
    // trigger imports-first.
    #[test]
    fn allows_jest_set_timeout_before_imports() {
        let src = r#"jest.setTimeout(180000);

import { PostResponse } from "lemmy-js-client/dist/types/PostResponse";
import { alpha, beta } from "./shared";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_vi_set_config_before_imports() {
        let src = r#"vi.setConfig({ testTimeout: 30000 });

import { describe, it } from "vitest";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_jasmine_timeout_before_imports() {
        let src = r#"jasmine.DEFAULT_TIMEOUT_INTERVAL = 10000;

import { Component } from "@angular/core";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_other_code_before_imports() {
        // A regular function call (not a known framework config) must still flag.
        let src = r#"console.log("hello");

import { a } from "a";
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
