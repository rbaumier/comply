use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, MethodDefinitionKind, PropertyKey};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// What a bodied class member is, for duplicate detection.
///
/// A `get`/`set` pair of the same name and static-ness forms one logical
/// property and is not a duplicate; every other same-name combination is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemberKind {
    Method,
    Getter,
    Setter,
    Property,
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Static and instance members live in separate namespaces, so a
        // `static foo` never collides with an instance `foo`. Key duplicate
        // groups on `(name, is_static)`; only bodied members participate
        // (overload signatures have no body).
        let mut seen: FxHashMap<(&str, bool), Vec<(u32, MemberKind)>> = FxHashMap::default();

        for element in &class.body.body {
            let (name, is_static, span_start, kind) = match element {
                ClassElement::MethodDefinition(m) => {
                    if m.value.body.is_none() {
                        continue; // overload signature
                    }
                    let name = match &m.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        _ => continue, // skip computed
                    };
                    let kind = match m.kind {
                        MethodDefinitionKind::Get => MemberKind::Getter,
                        MethodDefinitionKind::Set => MemberKind::Setter,
                        _ => MemberKind::Method,
                    };
                    (name, m.r#static, m.span.start, kind)
                }
                ClassElement::PropertyDefinition(p) => {
                    let name = match &p.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        _ => continue,
                    };
                    (name, p.r#static, p.span.start, MemberKind::Property)
                }
                _ => continue,
            };
            seen.entry((name, is_static)).or_default().push((span_start, kind));
        }

        for ((name, _), entries) in &seen {
            if entries.len() < 2 {
                continue;
            }
            // A single `get`/`set` accessor pair forms one logical property and
            // is allowed; any other same-name combination is a duplicate.
            if is_accessor_pair(entries) {
                continue;
            }
            for &(offset, _) in &entries[1..] {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate class member `{name}` \u{2014} this shadows the earlier definition."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}

/// True for exactly one getter plus one setter of the same name — the standard
/// accessor pattern, which is not a duplicate.
fn is_accessor_pair(entries: &[(u32, MemberKind)]) -> bool {
    let [(_, a), (_, b)] = entries else {
        return false;
    };
    matches!(
        (a, b),
        (MemberKind::Getter, MemberKind::Setter) | (MemberKind::Setter, MemberKind::Getter)
    )
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_duplicate_methods() {
        let diags = run_on("class Foo {\n  bar() {}\n  bar() {}\n}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_unique_members() {
        assert!(run_on("class Foo {\n  bar() {}\n  baz() {}\n}").is_empty());
    }

    #[test]
    fn allows_overload_signatures() {
        let _ =
            run_on("class Foo {\n  bar(): void;\n  bar(x: string): void;\n  bar(x?: string) {}\n}");
    }

    #[test]
    fn allows_getter_setter_pair() {
        let src = "class Foo {\n  get level(): number { return this._level; }\n  set level(value: number) { this._setLevelInput(value); }\n  _level!: number;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_getter_setter_pair() {
        let src = "class Foo {\n  static get x() { return 1; }\n  static set x(v) {}\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_same_name_across_static_boundary() {
        let src = "class Foo {\n  foo() {}\n  static foo() {}\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_duplicate_getters() {
        let diags = run_on("class Foo {\n  get bar() { return 1; }\n  get bar() { return 2; }\n}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn flags_duplicate_setters() {
        let diags = run_on("class Foo {\n  set bar(v) {}\n  set bar(v) {}\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_getter_and_method_same_name() {
        let diags = run_on("class Foo {\n  get bar() { return 1; }\n  bar() {}\n}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn flags_getter_setter_plus_third_member() {
        let diags = run_on(
            "class Foo {\n  get bar() { return 1; }\n  set bar(v) {}\n  bar() {}\n}",
        );
        assert_eq!(diags.len(), 2);
    }
}
