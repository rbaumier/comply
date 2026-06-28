//! OxcCheck backend for node-prefer-stream-pipeline.
//!
//! Flags a Node stream `.pipe()` call only when the result of its method chain
//! is discarded (the outermost chain expression sits in statement position).
//! `pipeline()` from `node:stream/promises` returns `Promise<void>`, not the
//! destination stream, so it can replace a `.pipe()` only when the chain's
//! result is not used. When the result is captured — assigned to a variable,
//! returned, passed as an argument, cast, or otherwise consumed — `pipeline()`
//! is not a valid substitute, so the call is left alone.
//!
//! File-level gating keeps the rule away from non-Node-stream `.pipe()`:
//! - `touches_node_streams` — only fire in files that import `stream`/
//!   `node:stream` or use `createReadStream`/`createWriteStream`;
//! - `is_gulp_vinyl_context` — never flag a Gulp build file's Vinyl `.pipe()`
//!   chains, which operate on file-object streams with no `pipeline()` analogue;
//! - `first_arg_is_functional_combinator` — skip effect-ts `.pipe(Effect.map(…))`
//!   combinator calls, which have nothing to do with Node streams.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".pipe("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Node-local shape check first (cheap, before any file scan): the callee
        // must be a `.pipe` member access.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "pipe" {
            return;
        }

        // File-level gating — only Node stream files, never Gulp Vinyl chains.
        if !touches_node_streams(ctx.source) {
            return;
        }
        if is_gulp_vinyl_context(ctx) {
            return;
        }

        // effect-ts `.pipe(Effect.map(...), ...)` is a functional combinator, not
        // a Node stream pipe — skip it.
        if let Some(first) = call.arguments.first() {
            let span = first.span();
            let text = &ctx.source[span.start as usize..span.end as usize];
            if first_arg_is_functional_combinator(text) {
                return;
            }
        }

        // Only flag a `.pipe()` whose chain result is discarded — `pipeline()`
        // returns `Promise<void>`, so it cannot stand in when the result is used.
        if !pipe_result_is_discarded(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Stream `.pipe()` does not destroy upstream/downstream on error — use \
                      `pipeline()` from `node:stream/promises` for automatic cleanup."
                .to_string(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the result of this `.pipe()` call's method chain is discarded:
/// climbing from the call to the outermost expression of its chain lands on an
/// `ExpressionStatement`. A `.pipe()` whose chain result is captured instead —
/// a variable initializer, a `return` argument, a call argument, a cast, an
/// `await`, etc. — reaches a non-statement parent and is not flagged.
///
/// The climb follows only chain links — a member access whose object is the
/// current node (`current.foo`), a call whose callee is the current node
/// (`current()`), or a parenthesized / non-null wrapper — so `a.pipe(b).pipe(c);`
/// resolves both inner and outer pipes to the same outermost chain expression
/// (its parent is the statement) and flags both. A node reached as a member
/// index (`obj[current]`) or a call argument (`fn(current)`) is not a chain
/// link: the climb stops there, the parent is not a statement, and the call is
/// left alone.
fn pipe_result_is_discarded<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let mut current_span = node.kind().span();
    loop {
        let parent = nodes.parent_node(current_id);
        match parent.kind() {
            AstKind::ExpressionStatement(_) => return true,
            AstKind::ParenthesizedExpression(p) => {
                current_span = p.span;
                current_id = parent.id();
            }
            AstKind::TSNonNullExpression(nn) => {
                current_span = nn.span;
                current_id = parent.id();
            }
            AstKind::StaticMemberExpression(m) if m.object.span() == current_span => {
                current_span = m.span;
                current_id = parent.id();
            }
            AstKind::ComputedMemberExpression(m) if m.object.span() == current_span => {
                current_span = m.span;
                current_id = parent.id();
            }
            AstKind::CallExpression(c) if c.callee.span() == current_span => {
                current_span = c.span;
                current_id = parent.id();
            }
            _ => return false,
        }
    }
}

/// Heuristic gate: only fire on files that genuinely touch Node streams.
/// Importing `fs` alone is not enough (`readFileSync` etc. are far more common
/// than streaming), so we require an explicit stream import or a stream-factory
/// call — the realistic source of a pipeable `Readable`/`Writable`.
fn touches_node_streams(source: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "from 'stream'",
        "from \"stream\"",
        "from 'node:stream'",
        "from \"node:stream\"",
        "require('stream')",
        "require(\"stream\")",
        "createReadStream",
        "createWriteStream",
    ];
    NEEDLES.iter().any(|n| crate::oxc_helpers::source_contains(source, n))
}

/// True when the file orchestrates a Gulp build. Gulp's `.pipe()` operates on
/// Vinyl file-object streams (`gulp.src(...)`, `gulp.dest(...)`) — a distinct
/// abstraction from Node `Readable`/`Writable` streams that has no
/// `stream.pipeline()` equivalent. The structural signal is that the chain
/// originates from Gulp: either the `gulp.src(` namespace call, or any import
/// of the `'gulp'` module (which is what makes a bare `src(...)` a Vinyl
/// source). The gulpfile filename is a complementary signal for the same intent.
fn is_gulp_vinyl_context(ctx: &CheckCtx) -> bool {
    crate::rules::path_utils::is_gulpfile(ctx.path)
        || ctx.source_contains("gulp.src(")
        || imports_gulp_module(ctx.source)
}

/// True when the file imports the `'gulp'` module — ESM
/// (`import gulp from 'gulp'`, `import { src } from 'gulp'`, possibly spanning
/// several lines) or CommonJS (`require('gulp')`). Matching the `'gulp'`
/// specifier next to `from`/`require` rather than a specific binding name keeps
/// multi-line named imports recognized without false-skipping a local `src`.
fn imports_gulp_module(source: &str) -> bool {
    const NEEDLES: &[&str] =
        &["from 'gulp'", "from \"gulp\"", "require('gulp')", "require(\"gulp\")"];
    NEEDLES.iter().any(|n| crate::oxc_helpers::source_contains(source, n))
}

/// True when the first argument of a `.pipe(` call is a functional combinator
/// (`Effect.map`, `Stream.tap`, `pipe(...)`, …). effect-ts uses `.pipe()` as
/// its core combinator — those calls have nothing to do with Node streams.
fn first_arg_is_functional_combinator(first_arg_text: &str) -> bool {
    let t = first_arg_text.trim_start();
    const COMBINATORS: &[&str] = &[
        "Effect.", "Stream.", "Sink.", "Layer.", "Schedule.", "Chunk.",
        "Option.", "Either.", "Exit.", "Fiber.", "STM.", "pipe(",
    ];
    COMBINATORS.iter().any(|c| t.starts_with(c))
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "io.ts")
    }

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_pipe_chain() {
        let src = "import { createReadStream, createWriteStream } from 'fs';\n\
                   createReadStream('a').pipe(createWriteStream('b'));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_pipes() {
        let src = "import { createReadStream } from 'node:fs';\n\
                   a.pipe(b).pipe(c);";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_pipeline_call() {
        // `pipeline(` is the recommendation — its callee is not a `.pipe` member,
        // so it is never matched.
        let src = "import { pipeline } from 'node:stream/promises';\n\
                   import { createReadStream } from 'fs';\n\
                   await pipeline(createReadStream('a'), createWriteStream('b'));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_files_without_streams() {
        let src = "obs.pipe(map(x => x + 1));";
        assert!(run(src).is_empty());
    }

    // Regression for #275: a file importing `fs` for non-stream reasons
    // (readFileSync) that uses Effect's `.pipe()` must not be flagged.
    #[test]
    fn skips_effect_pipe_with_plain_fs_import() {
        let src = "import { readFileSync } from 'fs';\n\
                   import { Effect } from 'effect';\n\
                   const p = eff.pipe(Effect.map(f), Effect.catchAll(g));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #275: even in a genuine stream file, Effect's `.pipe()`
    // combinator calls are spared — only the real stream pipe is flagged.
    #[test]
    fn flags_only_stream_pipe_alongside_effect() {
        let src = "import { createReadStream, createWriteStream } from 'node:fs';\n\
                   const program = eff.pipe(Effect.map(x => x), Effect.catchAll(h));\n\
                   createReadStream('a').pipe(createWriteStream('b'));";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #5075: Gulp's Vinyl `.pipe()` chain (gulp.src(...).pipe(...))
    // is not a Node stream — skipped via the structural `gulp.src(` signal even
    // when the file trips the stream gate (here via `createReadStream`).
    #[test]
    fn skips_gulp_src_vinyl_chain() {
        let src = "import gulp from 'gulp';\n\
                   import { createReadStream } from 'fs';\n\
                   gulp.src('src/pdf.js').pipe(rename('pdf.js')).pipe(gulp.dest('build'));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #5075: a bare `src(...)` chain is Vinyl only when `src` is
    // imported from `'gulp'`.
    #[test]
    fn skips_imported_src_vinyl_chain() {
        let src = "import { src, dest } from 'gulp';\n\
                   import { createReadStream } from 'fs';\n\
                   src('a').pipe(transform()).pipe(dest('build'));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #5075: the gulpfile filename is a complementary signal —
    // a `.pipe()` chain in `gulpfile.mjs` is skipped.
    #[test]
    fn skips_gulpfile_by_filename() {
        let src = "import { createReadStream, createWriteStream } from 'node:stream';\n\
                   build.src('a').pipe(b).pipe(c);";
        assert!(run_at("gulpfile.mjs", src).is_empty(), "{:?}", run_at("gulpfile.mjs", src));
    }

    // Regression for #5075: a multi-line named import from `'gulp'` (as
    // prettier formats it) still marks the file as a Gulp context.
    #[test]
    fn skips_multiline_gulp_import() {
        let src = "import {\n  src,\n  dest,\n} from 'gulp';\n\
                   import { createReadStream } from 'fs';\n\
                   src('a').pipe(transform()).pipe(dest('build'));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #5075: a bare `src(...)` NOT imported from gulp is a real
    // Node stream chain and must still be flagged — the gulp skip is precise.
    #[test]
    fn flags_non_gulp_src_named_base() {
        let src = "import { createReadStream, createWriteStream } from 'node:fs';\n\
                   const src = createReadStream('a');\n\
                   src.pipe(createWriteStream('b'));";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #6614: the `.pipe()` result is captured in a `const` and
    // used as the function's return value — `pipeline()` returns `Promise<void>`,
    // not the destination stream, so it cannot replace this pipe.
    #[test]
    fn allows_captured_pipe_assignment() {
        let src = "import { PassThrough } from 'node:stream';\n\
                   const stream = pack.pipe(new PassThrough());";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A `.pipe()` whose result is the function's return value is consumed —
    // the destination stream reference is needed, so it is not flagged.
    #[test]
    fn allows_returned_pipe() {
        let src = "import { PassThrough } from 'node:stream';\n\
                   function f(src, dest) { return src.pipe(dest); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A `.pipe()` whose result is passed as a call argument is consumed.
    #[test]
    fn allows_pipe_as_argument() {
        let src = "import { PassThrough } from 'node:stream';\n\
                   consume(src.pipe(dest));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A bare fire-and-forget `.pipe()` statement discards the result — exactly
    // the leaky pattern the rule targets — so it is still flagged.
    #[test]
    fn flags_fire_and_forget_pipe() {
        let src = "import { PassThrough } from 'node:stream';\n\
                   src.pipe(dest);";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
