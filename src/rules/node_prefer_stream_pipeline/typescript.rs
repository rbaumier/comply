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
}
