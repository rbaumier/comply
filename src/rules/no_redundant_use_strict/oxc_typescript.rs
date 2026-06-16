//! no-redundant-use-strict OXC backend — flag `"use strict"` directives that
//! have no effect.
//!
//! A `"use strict"` directive is redundant when the scope it sits in is already
//! strict:
//! - **ES modules** are always strict, so every `"use strict"` in a module is
//!   redundant.
//! - **Class bodies** are always strict, so a directive in a class method's body
//!   is redundant.
//! - A directive that an **enclosing scope's directive prologue** already
//!   established — an outer `"use strict"`, or an earlier duplicate in the same
//!   prologue — covers any nested directive.
//!
//! In a **script** (a `.cjs`/`.cts` file, or a `.js`/`.ts`/`.tsx` file inside a
//! `"type": "commonjs"` project) the single top-level `"use strict"` is *not*
//! redundant — it is the directive that turns strict mode on — so it never fires.
//!
//! Module-vs-script is decided per file: `.mjs`/`.mts` are always modules,
//! `.cjs`/`.cts` are always scripts, and the remaining JS/TS extensions are
//! modules unless the nearest `package.json` declares `"type": "commonjs"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::ModuleType;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Directive;
use std::sync::Arc;

const USE_STRICT: &str = "use strict";

pub struct Check;

/// True when the file is a *script* rather than an ES module. Scripts are the
/// only files where a top-level `"use strict"` is meaningful.
fn is_script_file(ctx: &CheckCtx) -> bool {
    match ctx.path.extension().and_then(|e| e.to_str()) {
        Some("cjs" | "cts") => true,
        Some("mjs" | "mts") => false,
        _ => ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.module_type == ModuleType::CommonJs),
    }
}

/// The strict-mode context that makes a `"use strict"` directive redundant,
/// resolved by walking outward from the directive. `Directive` carries the byte
/// span of the outer directive so the queried directive can recognise itself and
/// stay silent.
enum OuterStrict {
    Class,
    Directive(u32),
}

/// Resolve the outermost strict-mode context enclosing the directive at
/// `node_id` in a script file, mirroring Biome's ancestor walk.
///
/// Walks ancestors innermost-first, letting each enclosing strict context
/// overwrite the previous one so the *outermost* wins. The single top-level
/// directive of a script is exempt: it is the directive that enables strict
/// mode, so it is skipped when it is the last directive of the program prologue
/// and nothing stricter has been seen yet.
fn outermost_strict_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> Option<OuterStrict> {
    let mut outer: Option<OuterStrict> = None;
    for ancestor in semantic.nodes().ancestors(node_id) {
        match ancestor.kind() {
            AstKind::Class(_) => outer = Some(OuterStrict::Class),
            AstKind::Program(program) => {
                if let Some(directive) =
                    enclosing_strict_directive(&program.directives, true, outer.is_none())
                {
                    outer = Some(OuterStrict::Directive(directive));
                }
            }
            AstKind::FunctionBody(body) => {
                if let Some(directive) =
                    enclosing_strict_directive(&body.directives, false, outer.is_none())
                {
                    outer = Some(OuterStrict::Directive(directive));
                }
            }
            _ => {}
        }
    }
    outer
}

/// The span of the first `"use strict"` directive in a prologue that enables
/// strict mode for nested scopes, or `None` when the prologue has none (or only
/// the exempt top-level one).
///
/// `is_program` marks the script's top-level prologue; `no_outer_yet` is true
/// when no stricter context has been seen further out. The first `"use strict"`
/// of the program prologue is exempt only when it is also the last directive of
/// that prologue — a lone top-level directive turning strict mode on.
fn enclosing_strict_directive(
    directives: &[Directive],
    is_program: bool,
    no_outer_yet: bool,
) -> Option<u32> {
    let len = directives.len();
    for (index, directive) in directives.iter().enumerate() {
        if directive.expression.value.as_str() != USE_STRICT {
            continue;
        }
        if index + 1 == len && is_program && no_outer_yet {
            return None;
        }
        return Some(directive.span.start);
    }
    None
}

impl Check {
    fn push(&self, directive: &Directive, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
        let (line, column) = byte_offset_to_line_col(ctx.source, directive.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Redundant `\"use strict\"` directive.".into(),
            severity: Severity::Warning,
            span: Some((
                directive.span.start as usize,
                (directive.span.end - directive.span.start) as usize,
            )),
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Directive]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["use strict"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Directive(directive) = node.kind() else {
            return;
        };
        if directive.expression.value.as_str() != USE_STRICT {
            return;
        }

        // ES modules are always strict, so every `"use strict"` is redundant.
        if !is_script_file(ctx) {
            self.push(directive, ctx, diagnostics);
            return;
        }

        // Script: the directive is redundant only when an enclosing strict
        // context — a class body or an outer/earlier `"use strict"` — already
        // covers it. The lone top-level directive is the one that enables strict
        // mode, so `outermost_strict_context` exempts it.
        match outermost_strict_context(node.id(), semantic) {
            Some(OuterStrict::Class) => self.push(directive, ctx, diagnostics),
            Some(OuterStrict::Directive(span)) if span != directive.span.start => {
                self.push(directive, ctx, diagnostics);
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run against a path with the given extension; `source_type_for_path`
    /// derives module-vs-script from it, so `.js`/`.ts`/`.tsx` parse as modules
    /// and `.cjs` as a script (matching Biome's per-extension `JsFileSource`).
    fn run(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    /// Run a `.js` file inside a project whose `package.json` declares
    /// `"type": "commonjs"`, so the file is treated as a script.
    fn run_commonjs_js(src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"type":"commonjs"}"#).unwrap();
        let file_path = dir.path().join("index.js");
        fs::write(&file_path, src).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let project = ProjectCtx::load(&refs, &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    fn lines(diags: &[Diagnostic]) -> Vec<usize> {
        diags.iter().map(|d| d.line).collect()
    }

    // ── Biome invalid.cjs (script) ──────────────────────────────────────
    // Top-level duplicate fires (line 2, not line 1); the nested directives
    // are each covered by the outer top-level directive.
    #[test]
    fn invalid_cjs() {
        let src = "\"use strict\";\n\"use strict\";\n\nfunction test() {\n\t\"use strict\";\n\tfunction inner_a() {\n\t\t\"use strict\"; // redundant directive\n\t}\n\tfunction inner_b() {\n\t\tfunction inner_inner() {\n\t\t\t\"use strict\"; // additional redundant directive\n\t\t}\n\t}\n}\n";
        assert_eq!(lines(&run(src, "invalid.cjs")), vec![2, 5, 7, 11]);
    }

    // ── Biome invalid.js (module) ───────────────────────────────────────
    // Every directive is redundant in a module, including the top-level one.
    #[test]
    fn invalid_js_module() {
        let src = "// js module\n\"use strict\"; // Associated comment\n\nfunction foo() {\n\t\"use strict\";\n}\n\nclass C1 {\n\t// All code here is evaluated in strict mode\n\ttest() {\n\t\t\"use strict\";\n\t}\n}\n\nconst C2 = class {\n\t// All code here is evaluated in strict mode\n\ttest() {\n\t\t\"use strict\";\n\t}\n};\n";
        assert_eq!(lines(&run(src, "invalid.js")), vec![2, 5, 11, 18]);
    }

    // ── Biome invalid.ts (module) ───────────────────────────────────────
    #[test]
    fn invalid_ts_module() {
        let src = "function test(): void {\n\t\"use strict\";\n}\n";
        assert_eq!(lines(&run(src, "invalid.ts")), vec![2]);
    }

    // ── Biome invalidClass.cjs (script) ─────────────────────────────────
    // Class bodies are always strict, so directives inside class methods fire
    // even in a script with no top-level directive.
    #[test]
    fn invalid_class_cjs() {
        let src = "class C1 {\n\ttest() {\n\t\t\"use strict\";\n\t}\n}\n\nconst C2 = class {\n\ttest() {\n\t\t\"use strict\";\n\t}\n};\n";
        assert_eq!(lines(&run(src, "invalidClass.cjs")), vec![3, 9]);
    }

    // ── Biome invalidFunction.cjs (script) ──────────────────────────────
    // Two directives in one function prologue: the second is the duplicate.
    #[test]
    fn invalid_function_cjs() {
        let src = "function test() {\n\t\"use strict\";\n\t\"use strict\";\n}\n";
        assert_eq!(lines(&run(src, "invalidFunction.cjs")), vec![3]);
    }

    // ── Biome invalidFunction.js (module) ───────────────────────────────
    // In a module both directives fire.
    #[test]
    fn invalid_function_js_module() {
        let src = "function test() {\n\t\"use strict\";\n\t\"use strict\";\n}\n";
        assert_eq!(lines(&run(src, "invalidFunction.js")), vec![2, 3]);
    }

    // ── Biome invalid-with-trivia.js (module) ───────────────────────────
    // Leading comments/reference directive don't change anything: the lone
    // top-level directive is still redundant in a module.
    #[test]
    fn invalid_with_trivia_js_module() {
        let src = "/// <reference types=\"node\" />\n// comment\n// comment\n// comment\n\"use strict\" // comment\n\nlet foo = \"foo\"\n";
        assert_eq!(lines(&run(src, "invalid-with-trivia.js")), vec![5]);
    }

    // ── Biome valid.cjs (script) ────────────────────────────────────────
    // A single top-level directive in each function — none enclosed by a
    // stricter context — so nothing is redundant.
    #[test]
    fn valid_cjs() {
        let src = "/* should not generate diagnostics */\nfunction foo() {\n\t\"use strict\";\n}\nfunction bar() {\n\t\"use strict\";\n}\n";
        assert!(run(src, "valid.cjs").is_empty());
    }

    // ── Biome validReactDirectives.tsx (module) ─────────────────────────
    // `'use client'` is not `"use strict"`, so it never fires.
    #[test]
    fn valid_react_directives_tsx() {
        let src = "/* should not generate diagnostics */\n'use client'\n\nimport { useState } from \"react\"\n\nexport default function Counter() {\n  const [count, setCount] = useState(0)\n  return count\n}\n";
        assert!(run(src, "validReactDirectives.tsx").is_empty());
    }

    // ── Biome commonJsValid.js (script via package.json "type":"commonjs") ─
    // A single top-level directive in a CommonJS `.js` file is not redundant.
    #[test]
    fn common_js_valid_js() {
        let src = "/* should not generate diagnostics */\n\"use strict\"\n\nconst a = require(\"a\")\n";
        assert!(run_commonjs_js(src).is_empty(), "{:?}", run_commonjs_js(src));
    }

    // ── Extra coverage ──────────────────────────────────────────────────

    // The lone top-level directive in a script `.cjs` is valid (enables strict
    // mode), confirming the exemption.
    #[test]
    fn allows_single_top_level_directive_in_script() {
        assert!(run("\"use strict\";\nconst a = 1;\n", "t.cjs").is_empty());
    }

    // The same lone top-level directive is redundant in a module.
    #[test]
    fn flags_single_top_level_directive_in_module() {
        assert_eq!(lines(&run("\"use strict\";\nconst a = 1;\n", "t.js")), vec![1]);
    }

    // A directive that isn't `"use strict"` is never flagged.
    #[test]
    fn ignores_other_directives() {
        assert!(run("\"use server\";\nexport const a = 1;\n", "t.ts").is_empty());
    }

    // `.mjs` is always a module, so its top-level directive is redundant even
    // without a package.json.
    #[test]
    fn flags_top_level_directive_in_mjs() {
        assert_eq!(lines(&run("\"use strict\";\nexport const a = 1;\n", "t.mjs")), vec![1]);
    }
}
