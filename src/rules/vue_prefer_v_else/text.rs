use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for i in 0..lines.len().saturating_sub(1) {
            let cur = lines[i].trim();
            let next = lines[i + 1].trim();
            if !cur.contains("v-if=") || !next.contains("v-if=") {
                continue;
            }
            if let (Some(cur_cond), Some(next_cond)) = (extract_v_if(cur), extract_v_if(next))
                && (next_cond == format!("!{cur_cond}")
                    || next_cond == format!("!({cur_cond})"))
            {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 2,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Replace `v-if=\"{next_cond}\"` with `v-else` since the previous element uses `v-if=\"{cur_cond}\"`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

fn extract_v_if(line: &str) -> Option<String> {
    let start = line.find("v-if=\"")?;
    let after = &line[start + 6..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_negated_v_if() {
        assert_eq!(
            run("<div v-if=\"show\" />\n<div v-if=\"!show\" />").len(),
            1
        );
    }
    #[test]
    fn allows_v_else() {
        assert!(run("<div v-if=\"show\" />\n<div v-else />").is_empty());
    }
}
