use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ARRAY_METHODS: &[&str] = &[
    ".map(",
    ".filter(",
    ".reduce(",
    ".find(",
    ".some(",
    ".every(",
    ".flatMap(",
];

/// Check if a line starts an array method callback with a block body (`=> {`).
fn starts_block_callback(line: &str) -> bool {
    let has_method = ARRAY_METHODS.iter().any(|m| line.contains(m));
    has_method && line.contains("=> {")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let len = lines.len();

        let mut i = 0;
        while i < len {
            if starts_block_callback(lines[i]) {
                // Scan up to 10 lines ahead for `return` or `}`.
                let mut found_return = false;
                let end = (i + 11).min(len);
                for line in lines.iter().take(end).skip(i + 1) {
                    let trimmed = line.trim();
                    if trimmed.contains("return ")
                        || trimmed.starts_with("return;")
                        || trimmed == "return"
                    {
                        found_return = true;
                        break;
                    }
                    if trimmed.starts_with('}')
                        || trimmed == "}"
                        || trimmed == "});"
                        || trimmed == "})"
                    {
                        break;
                    }
                }
                if !found_return {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: "array-callback-without-return".into(),
                        message: "Array method callback uses block body `=> { ... }` without a `return` statement.".into(),
                        severity: Severity::Error,
                    });
                }
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
    fn flags_map_without_return() {
        let src = r#"const x = arr.map((item) => {
  console.log(item);
});"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_filter_without_return() {
        let src = r#"const x = arr.filter((item) => {
  item > 0;
});"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_with_return() {
        let src = r#"const x = arr.map((item) => {
  return item * 2;
});"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_concise_arrow() {
        let src = "const x = arr.map((item) => item * 2);";
        assert!(run(src).is_empty());
    }
}
