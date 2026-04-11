use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Patterns: borrowing methods from literal instances instead of prototypes.
///
/// Flags:
/// - `{}.hasOwnProperty.call(` → `Object.prototype.hasOwnProperty.call(`
/// - `{}.isPrototypeOf.call(` → `Object.prototype.isPrototypeOf.call(`
/// - `{}.propertyIsEnumerable.call(` → `Object.prototype.propertyIsEnumerable.call(`
/// - `{}.toString.call(` → `Object.prototype.toString.call(`
/// - `{}.valueOf.call(` → `Object.prototype.valueOf.call(`
/// - `[].slice.call(` → `Array.prototype.slice.call(`
/// - `[].map.call(` → `Array.prototype.map.call(`
/// - `[].forEach.call(` → `Array.prototype.forEach.call(`
///   etc.
const OBJECT_LITERAL_PATTERNS: &[(&str, &str)] = &[
    (
        "{}.hasOwnProperty.call(",
        "Object.prototype.hasOwnProperty.call(",
    ),
    (
        "{}.isPrototypeOf.call(",
        "Object.prototype.isPrototypeOf.call(",
    ),
    (
        "{}.propertyIsEnumerable.call(",
        "Object.prototype.propertyIsEnumerable.call(",
    ),
    (
        "{}.toLocaleString.call(",
        "Object.prototype.toLocaleString.call(",
    ),
    ("{}.toString.call(", "Object.prototype.toString.call("),
    ("{}.valueOf.call(", "Object.prototype.valueOf.call("),
    (
        "{}.hasOwnProperty.apply(",
        "Object.prototype.hasOwnProperty.apply(",
    ),
    ("{}.toString.apply(", "Object.prototype.toString.apply("),
    (
        "{}.hasOwnProperty.bind(",
        "Object.prototype.hasOwnProperty.bind(",
    ),
    ("{}.toString.bind(", "Object.prototype.toString.bind("),
];

const ARRAY_LITERAL_PATTERNS: &[(&str, &str)] = &[
    ("[].slice.call(", "Array.prototype.slice.call("),
    ("[].map.call(", "Array.prototype.map.call("),
    ("[].forEach.call(", "Array.prototype.forEach.call("),
    ("[].filter.call(", "Array.prototype.filter.call("),
    ("[].concat.call(", "Array.prototype.concat.call("),
    ("[].indexOf.call(", "Array.prototype.indexOf.call("),
    ("[].join.call(", "Array.prototype.join.call("),
    ("[].push.call(", "Array.prototype.push.call("),
    ("[].splice.call(", "Array.prototype.splice.call("),
    ("[].reduce.call(", "Array.prototype.reduce.call("),
    ("[].find.call(", "Array.prototype.find.call("),
    ("[].includes.call(", "Array.prototype.includes.call("),
    ("[].some.call(", "Array.prototype.some.call("),
    ("[].every.call(", "Array.prototype.every.call("),
    ("[].flat.call(", "Array.prototype.flat.call("),
    ("[].flatMap.call(", "Array.prototype.flatMap.call("),
    ("[].slice.apply(", "Array.prototype.slice.apply("),
    ("[].slice.bind(", "Array.prototype.slice.bind("),
];

fn find_prototype_violation(line: &str) -> Option<String> {
    for &(pattern, replacement) in OBJECT_LITERAL_PATTERNS.iter().chain(ARRAY_LITERAL_PATTERNS) {
        if line.contains(pattern) {
            let constructor = if pattern.starts_with("{}") {
                "Object"
            } else {
                "Array"
            };
            // Extract method name from pattern: `{}.hasOwnProperty.call(` → `hasOwnProperty`
            let method = pattern
                .trim_start_matches("{}")
                .trim_start_matches("[]")
                .trim_start_matches('.')
                .split('.')
                .next()
                .unwrap_or("?");
            return Some(format!(
                "Prefer `{replacement}…)` over `{pattern}…)`. \
                 Borrow the method from `{constructor}.prototype.{method}` instead of a literal instance."
            ));
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_prototype_violation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-prototype-methods".into(),
                    message: msg,
                    severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_object_has_own_property_call() {
        let d = run("const has = {}.hasOwnProperty.call(obj, 'key');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.hasOwnProperty"));
    }

    #[test]
    fn flags_object_to_string_call() {
        let d = run("const type = {}.toString.call(value);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.toString"));
    }

    #[test]
    fn flags_array_slice_call() {
        let d = run("const args = [].slice.call(arguments);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.slice"));
    }

    #[test]
    fn flags_array_map_call() {
        let d = run("[].map.call(nodeList, fn)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.map"));
    }

    #[test]
    fn allows_prototype_methods() {
        assert!(run("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }

    #[test]
    fn allows_array_prototype_methods() {
        assert!(run("Array.prototype.slice.call(arguments)").is_empty());
    }

    #[test]
    fn allows_normal_method_calls() {
        assert!(run("obj.hasOwnProperty('key')").is_empty());
    }
}
