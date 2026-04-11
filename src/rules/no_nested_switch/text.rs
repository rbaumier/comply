use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let src = ctx.source;
        let bytes = src.as_bytes();
        let len = bytes.len();

        // Track switch depth via brace counting.
        // Each time we see `switch`, push current brace depth onto a stack.
        // When we pop back to that depth, we've exited that switch.
        let mut switch_depth: usize = 0;
        let mut brace_targets: Vec<usize> = Vec::new(); // brace depth at which each switch started
        let mut brace_depth: usize = 0;
        let mut i = 0;

        while i < len {
            let b = bytes[i];

            // Skip string literals.
            if b == b'"' || b == b'\'' || b == b'`' {
                let quote = b;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            // Skip single-line comments.
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Skip multi-line comments.
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            if b == b'{' {
                brace_depth += 1;
            } else if b == b'}' {
                if brace_depth > 0 {
                    brace_depth -= 1;
                }
                // Check if we're exiting a switch block.
                while let Some(&target) = brace_targets.last() {
                    if brace_depth <= target {
                        brace_targets.pop();
                        switch_depth -= 1;
                    } else {
                        break;
                    }
                }
            }

            // Detect `switch` keyword.
            if b == b's'
                && i + 6 < len
                && &src[i..i + 6] == "switch"
                && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_')
                && !bytes[i + 6].is_ascii_alphanumeric()
                && bytes[i + 6] != b'_'
            {
                if switch_depth > 0 {
                    let line = src[..i].matches('\n').count() + 1;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line,
                        column: 1,
                        rule_id: "no-nested-switch".into(),
                        message: "Nested `switch` — extract the inner switch into a separate function.".into(),
                        severity: Severity::Error,
                    });
                }
                switch_depth += 1;
                brace_targets.push(brace_depth);
                i += 6;
                continue;
            }

            i += 1;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_nested_switch() {
        let src = r#"
switch (a) {
  case 1:
    switch (b) {
      case 2: break;
    }
    break;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sequential_switches() {
        let src = r#"
switch (a) {
  case 1: break;
}
switch (b) {
  case 2: break;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_switch() {
        let src = r#"
switch (action) {
  case "start": run(); break;
  case "stop": halt(); break;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_switch() {
        let src = r#"
switch (a) {
  case 1:
    switch (b) {
      case 2:
        switch (c) {
          case 3: break;
        }
        break;
    }
    break;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }
}
