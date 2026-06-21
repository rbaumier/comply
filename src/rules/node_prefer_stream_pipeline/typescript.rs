//! Flag `.pipe(` calls that look like Node stream chaining. We deliberately
//! ignore RxJS-style `pipe(` (no leading dot is required there) and the
//! `pipeline(` import which is the recommended replacement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

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
    source.contains("from 'gulp'")
        || source.contains("from \"gulp\"")
        || source.contains("require('gulp')")
        || source.contains("require(\"gulp\")")
}

/// True when the first argument of a `.pipe(` call is a functional combinator
/// (`Effect.map`, `Stream.tap`, `pipe(...)`, …). effect-ts uses `.pipe()` as
/// its core combinator — those calls have nothing to do with Node streams.
fn first_arg_is_functional_combinator(rest_after_paren: &str) -> bool {
    let t = rest_after_paren.trim_start();
    const COMBINATORS: &[&str] = &[
        "Effect.", "Stream.", "Sink.", "Layer.", "Schedule.", "Chunk.",
        "Option.", "Either.", "Exit.", "Fiber.", "STM.", "pipe(",
    ];
    COMBINATORS.iter().any(|c| t.starts_with(c))
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".pipe("])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !touches_node_streams(ctx.source) {
            return Vec::new();
        }
        // Gulp's Vinyl `.pipe()` is not a Node stream and has no `pipeline()`
        // equivalent — never flag a Gulp build file's chains.
        if is_gulp_vinyl_context(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, raw_line) in ctx.source.lines().enumerate() {
            // Strip line comments.
            let line = match raw_line.find("//") {
                Some(p) => &raw_line[..p],
                None => raw_line,
            };
            let bytes = line.as_bytes();
            let mut i = 0;
            while i + 5 <= bytes.len() {
                if &bytes[i..i + 5] == b".pipe" {
                    // Must be followed by `(`.
                    let after = i + 5;
                    if after < bytes.len() && bytes[after] == b'(' {
                        // effect-ts `.pipe(Effect.map(...), ...)` is a functional
                        // pipeline, not a Node stream — skip it.
                        if first_arg_is_functional_combinator(&line[after + 1..]) {
                            i = after + 1;
                            continue;
                        }
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: i + 1,
                            rule_id: super::META.id.into(),
                            message: "Stream `.pipe()` does not destroy upstream/downstream on \
                                      error — use `pipeline()` from `node:stream/promises` for \
                                      automatic cleanup."
                                .to_string(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        i = after + 1;
                        continue;
                    }
                }
                i += 1;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("io.ts"), source))
    }

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
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
        // `pipeline(` is the recommendation — never .pipe-prefixed, so it's
        // not matched by `.pipe(`.
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
}
