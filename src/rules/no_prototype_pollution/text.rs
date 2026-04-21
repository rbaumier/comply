use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MERGE_FNS: &[&str] = &[
    "_.merge(",
    "deepMerge(",
    "lodash.merge(",
    "mergeDeep(",
    "Object.assign(",
];
const USER_DATA: &[&str] = &["req.body", "request.body", "JSON.parse("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !MERGE_FNS.iter().any(|m| t.contains(m)) {
                continue;
            }
            if USER_DATA.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-prototype-pollution".into(),
                    message: "Deep-merging user-controlled data risks prototype pollution — sanitize input before merging.".into(),
                    severity: Severity::Error,
                    span: None,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_lodash_merge_req_body() {
        assert_eq!(run("_.merge(config, req.body)").len(), 1);
    }
    #[test]
    fn flags_merge_with_json_parse() {
        assert_eq!(run("deepMerge(defaults, JSON.parse(raw))").len(), 1);
    }
    #[test]
    fn flags_object_assign_req_body() {
        assert_eq!(run("Object.assign(target, req.body)").len(), 1);
    }
    #[test]
    fn allows_merge_safe_data() {
        assert!(run("_.merge(config, defaults)").is_empty());
    }
}
