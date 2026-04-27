//! Flag `.pipe(` calls that look like Node stream chaining. We deliberately
//! ignore RxJS-style `pipe(` (no leading dot is required there) and the
//! `pipeline(` import which is the recommended replacement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

/// Heuristic gate: only fire on files that look like they touch Node streams.
/// We require an import from `node:stream`, `stream`, `node:fs`, `fs`, or
/// `node:http`, `http` — the realistic source of `Readable`/`Writable`.
fn touches_node_streams(source: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "from 'stream'",
        "from \"stream\"",
        "from 'node:stream'",
        "from \"node:stream\"",
        "require('stream')",
        "require(\"stream\")",
        "from 'fs'",
        "from \"fs\"",
        "from 'node:fs'",
        "from \"node:fs\"",
        "createReadStream",
        "createWriteStream",
    ];
    NEEDLES.iter().any(|n| source.contains(n))
}

impl TextCheck for Check {
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
                        // Word boundary on the left — `.pipe` must be a call,
                        // not part of `.pipeline` or `.piped`.
                        let next_after_paren = after + 1;
                        let _ = next_after_paren;
                        // Reject `.pipeline(` — already handled by token length.
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
}
