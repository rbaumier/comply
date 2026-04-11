use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `{identifier && <` patterns where identifier is not a boolean expression.
/// Flags: `{count && <X />}`, `{items.length && <X />}`
/// Allows: `{!!count && <X />}`, `{count > 0 && <X />}`, `{isReady && <X />}`
fn has_leaked_render(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }

        let brace_pos = i;
        i += 1;

        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // If we see `!!`, it's a boolean coercion — skip
        if i + 1 < len && bytes[i] == b'!' && bytes[i + 1] == b'!' {
            i += 2;
            continue;
        }

        // Collect the identifier (may include `.` for `x.length`)
        let id_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
        {
            i += 1;
        }
        let id_end = i;
        if id_start == id_end {
            continue;
        }

        let ident = &line[id_start..id_end];

        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // Check for comparison operators — these produce booleans, so they're safe
        if i < len && (bytes[i] == b'>' || bytes[i] == b'<' || bytes[i] == b'=' || bytes[i] == b'!')
        {
            continue;
        }

        // Must be followed by `&&`
        if i + 1 < len && bytes[i] == b'&' && bytes[i + 1] == b'&' {
            i += 2;

            // Skip whitespace
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            // Must be followed by `<` (JSX element)
            if i < len && bytes[i] == b'<' {
                // Skip identifiers starting with `is`, `has`, `should`, `can` — likely booleans
                let lower = ident.to_lowercase();
                let last_segment = lower.rsplit('.').next().unwrap_or(&lower);
                let likely_boolean = last_segment.starts_with("is")
                    || last_segment.starts_with("has")
                    || last_segment.starts_with("should")
                    || last_segment.starts_with("can")
                    || last_segment.starts_with("will")
                    || last_segment.starts_with("did")
                    || last_segment.starts_with("show")
                    || last_segment.starts_with("hide")
                    || last_segment.starts_with("enable")
                    || last_segment.starts_with("disable")
                    || last_segment.starts_with("visible")
                    || last_segment.starts_with("active")
                    || last_segment.starts_with("open")
                    || last_segment.starts_with("loading")
                    || last_segment.starts_with("loaded");

                if !likely_boolean {
                    hits.push(brace_pos + 1); // 1-based column
                }
            }
        }
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for _col in has_leaked_render(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "jsx-no-leaked-render".into(),
                    message: "Potential leaked render — numeric/string value with `&&` renders falsy value (`0`, `\"\"`) instead of nothing.".into(),
                    severity: Severity::Error,
                });
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_count_and_jsx() {
        assert_eq!(run("  return {count && <Component />};").len(), 1);
    }

    #[test]
    fn flags_length_and_jsx() {
        assert_eq!(run("  return {items.length && <List />};").len(), 1);
    }

    #[test]
    fn allows_double_bang() {
        assert!(run("  return {!!count && <Component />};").is_empty());
    }

    #[test]
    fn allows_comparison() {
        assert!(run("  return {count > 0 && <Component />};").is_empty());
    }

    #[test]
    fn allows_boolean_prefix() {
        assert!(run("  return {isReady && <Component />};").is_empty());
    }
}
