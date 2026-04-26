//! no-prototype-pollution backend — flag deep-merge / Object.assign calls
//! whose source argument is a user-controlled payload (`req.body`,
//! `request.body`, `JSON.parse(...)`).

use crate::diagnostic::{Diagnostic, Severity};

const MERGE_CALLS: &[&str] = &[
    "_.merge",
    "lodash.merge",
    "deepMerge",
    "mergeDeep",
    "Object.assign",
];

const USER_DATA_NEEDLES: &[&str] = &["req.body", "request.body", "JSON.parse"];

fn looks_like_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    let matches_merge = MERGE_CALLS.iter().any(|m| name == *m || name.ends_with(&format!(".{m}")));
    if !matches_merge {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let mut tainted = false;
    for arg in args.named_children(&mut cursor) {
        let Ok(text) = arg.utf8_text(source) else { continue };
        if looks_like_user_data(text) {
            tainted = true;
            break;
        }
    }
    if !tainted {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-prototype-pollution",
        "Deep-merging user-controlled data risks prototype pollution — sanitize input before merging.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_lodash_merge_req_body() {
        assert_eq!(run_on("_.merge(config, req.body)").len(), 1);
    }

    #[test]
    fn flags_merge_with_json_parse() {
        assert_eq!(run_on("deepMerge(defaults, JSON.parse(raw))").len(), 1);
    }

    #[test]
    fn flags_object_assign_req_body() {
        assert_eq!(run_on("Object.assign(target, req.body)").len(), 1);
    }

    #[test]
    fn allows_merge_safe_data() {
        assert!(run_on("_.merge(config, defaults)").is_empty());
    }

    #[test]
    fn allows_unrelated_call() {
        assert!(run_on("add(a, req.body)").is_empty());
    }
}
