//! zod-string-min-1-required: flag `z.string()` calls without a length/format/optionality continuation.
//! Skipped in test files: fixtures use `z.string()` as a stand-in stub, never `.parse()`d at runtime.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e.", ".e2e-spec."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case("tests") || name.eq_ignore_ascii_case("e2e")
    })
}

/// PascalCase words that mark a schema as a response/wire-contract
/// (server-emitted) shape rather than a user-input schema. The server controls
/// the wire format, so `z.string()` fields need not be non-empty.
const RESPONSE_SCHEMA_MARKERS: &[&str] = &[
    "Response", "Output", "Result", "Reply", "Wire",
    "Dto", "DTO", "Error", "Problem",
];

/// PascalCase words that mark a schema as user/wire **input** — a POST/PATCH
/// body, a URL query, a filter, or a form — where an empty `z.string()` is a
/// real defect the rule must keep flagging. These take precedence over the
/// `<Entity>Schema` response convention below: `Edit<Entity>InputSchema` is
/// input even though it ends in `Schema`.
const INPUT_SCHEMA_MARKERS: &[&str] = &[
    "Input", "Body", "Payload", "Request", "Args", "Params",
    "Form", "Filters", "Query", "Config",
];

/// True when `z.string()` lives inside a variable whose name marks it as a
/// response / wire-read shape rather than a user-input schema.
///
/// A schema is treated as a response when its declarator name carries an
/// explicit response marker (`ProblemSchema`, `FooResponseSchema`, `UserDto`)
/// **or** follows the canonical entity-mirror convention — a PascalCase
/// identifier ending in `Schema` (`TeamCentralCodeSchema`), the shape that
/// deserializes a server-emitted row (issue #513). An input marker
/// (`…InputSchema`, `…QuerySchema`, `…FiltersSchema`) overrides both: input
/// bodies keep needing `.min(1)`, so they stay flagged.
fn is_inside_response_schema(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::VariableDeclarator(decl) = ancestor.kind() {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id else {
                return false;
            };
            return is_response_schema_name(&id.name);
        }
    }
    false
}

/// A schema name is a response/wire-read shape when it is not input-direction
/// and either carries a response marker or matches the `<Entity>Schema`
/// entity-mirror convention.
fn is_response_schema_name(name: &str) -> bool {
    if INPUT_SCHEMA_MARKERS.iter().any(|m| contains_pascal_word(name, m)) {
        return false;
    }
    RESPONSE_SCHEMA_MARKERS.iter().any(|m| contains_pascal_word(name, m))
        || is_entity_mirror_schema_name(name)
}

/// True when `word` (itself a PascalCase token, first char uppercase) appears
/// in `name` as a whole PascalCase segment, not merely as a substring. The
/// segment ends at a new word — end of string, an uppercase letter, or a digit
/// — so `Form` matches `FormSchema`/`EditFormSchema` but not `FormatSchema`,
/// and `Config` matches `EmailConfigSchema` but not `ConfigurationSchema`. The
/// leading boundary is implicit: `word` starts uppercase, which already opens a
/// PascalCase segment wherever it occurs.
fn contains_pascal_word(name: &str, word: &str) -> bool {
    name.match_indices(word).any(|(index, _)| {
        match name[index + word.len()..].chars().next() {
            None => true,
            Some(next) => next.is_ascii_uppercase() || next.is_ascii_digit(),
        }
    })
}

/// The canonical wire-read convention: a PascalCase identifier ending in
/// `Schema` (`TeamCentralCodeSchema`, `ProductSchema`). Requiring PascalCase
/// keeps ad-hoc camelCase input schemas (`loginSchema`) flagged: those are
/// hand-named form bodies, not the generated entity-row mirror.
fn is_entity_mirror_schema_name(name: &str) -> bool {
    name.ends_with("Schema") && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

const VALID_CONTINUATIONS: &[&str] = &[
    "min",
    "max",
    "email",
    "url",
    "uuid",
    "regex",
    "length",
    "startsWith",
    "endsWith",
    "optional",
    "nullable",
    "nullish",
    "trim",
    "toLowerCase",
    "toUpperCase",
];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// Check if this call expression is the object of a parent member expression
/// with a valid continuation method. We do this by checking if this call is
/// wrapped in a `z.string().min(1)` style chain via the source text around
/// the call span.
fn is_chained_with_valid_continuation(call_end: u32, source: &str) -> bool {
    let rest = &source[call_end as usize..];
    let trimmed = rest.trim_start();
    if let Some(after_dot) = trimmed.strip_prefix('.') {
        let method: String = after_dot
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        return VALID_CONTINUATIONS.contains(&method.as_str());
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if is_test_file(ctx.path) {
            return;
        }

        if is_inside_response_schema(node, semantic) {
            return;
        }

        let Some(name) = callee_name(&call.callee) else { return };
        if name != "z.string" {
            return;
        }

        // Check if this z.string() is chained with a valid continuation.
        if is_chained_with_valid_continuation(call.span.end, ctx.source) {
            return;
        }

        // z.string() passed directly as an argument to a function: the wrapper
        // may apply constraints internally (e.g. refineNoControlChars adds .min(1)).
        if matches!(semantic.nodes().parent_node(node.id()).kind(), AstKind::CallExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `z.string()` accepts empty strings \u{2014} add `.min(1)` or a format constraint.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
    }

    #[test]
    fn flags_bare_string() {
        assert_eq!(run("const s = z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_min() {
        assert!(run("z.string().min(1)").is_empty());
    }

    #[test]
    fn allows_email() {
        assert!(run("z.string().email()").is_empty());
    }

    #[test]
    fn allows_optional() {
        assert!(run("z.string().optional()").is_empty());
    }

    #[test]
    fn no_fp_when_passed_to_wrapper_function() {
        // Regression for issue #428: z.string() passed to a helper that applies .min(1) internally.
        assert!(run("refineNoControlChars(z.string(), 'label')").is_empty());
        assert!(run("refineNoControlChars(z.string(), fieldLabel)").is_empty());
        assert!(run("const s = refineNoControlChars(z.string(), 'x')").is_empty());
    }

    #[test]
    fn still_flags_bare_string_in_object() {
        // z.string() inside an object literal is not passed to a wrapper — still flagged.
        assert_eq!(run("z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_bare_string_in_test_file() {
        // Regression for issue #119: `z.string()` in test fixtures is a
        // typed stand-in that is never `.parse()`d at runtime.
        let code = "const schema = z.object({ sort: z.string() });";
        assert!(run_at(code, "src/foo.test.ts").is_empty());
        assert!(run_at(code, "src/foo.spec.ts").is_empty());
        assert!(run_at(code, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(code, "e2e/foo.ts").is_empty());
        assert!(run_at(code, "tests/foo.ts").is_empty());
        assert!(run_at(code, "src/foo.e2e-spec.ts").is_empty());
        assert!(run_at(code, "src/foo_test.ts").is_empty());
    }

    #[test]
    fn no_fp_on_response_wire_contract_schema() {
        // Regression for issue #513: RFC 7807 Problem JSON response schema —
        // z.string() fields in response/wire-contract schemas must not require
        // .min(1) because the server may legitimately emit empty strings.
        let rfc7807 = r#"
            export const ProblemSchema = z.object({
                type: z.string(),
                title: z.string(),
                status: z.number(),
                detail: z.string(),
                instance: z.string(),
            });
        "#;
        assert!(run(rfc7807).is_empty());

        // Other common response-schema naming conventions.
        assert!(run("const FooResponseSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooResponse = z.object({ name: z.string() });").is_empty());
        assert!(run("const FooOutputSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const UserDto = z.object({ name: z.string() });").is_empty());
        assert!(run("const ApiErrorSchema = z.object({ message: z.string() });").is_empty());
        assert!(run("const SearchResult = z.object({ label: z.string() });").is_empty());
    }

    #[test]
    fn no_fp_on_entity_mirror_schema() {
        // Regression for the #513 reopening: the canonical wire-read shape is
        // a PascalCase `<Entity>Schema`, which carries none of the explicit
        // response markers above yet still deserializes a server-emitted row.
        // The exact reproducer from the issue (TeamCentralCodeSchema.code).
        let entity_mirror = r#"
            export const TeamCentralCodeSchema = z.object({
                id: TeamCentralCodeIdSchema,
                teamId: TeamIdSchema,
                centraleId: CentraleIdSchema,
                code: z.string(),
                isWeb: z.boolean(),
                deactivatedAt: z.coerce.date().nullable(),
                createdAt: z.coerce.date(),
            });
        "#;
        assert!(run(entity_mirror).is_empty());

        // Other plain entity-mirror names from the same convention.
        assert!(run("const ProductSchema = z.object({ name: z.string() });").is_empty());
        assert!(run("const LaboratorySchema = z.object({ label: z.string() });").is_empty());
        // Extended / derived response shapes follow the same convention.
        assert!(
            run("const TransverseTeamCentralCodeSchema = z.object({ code: z.string() });").is_empty()
        );
    }

    #[test]
    fn input_markers_match_whole_pascal_words_not_substrings() {
        // Markers are matched on PascalCase word boundaries, so an entity name
        // that merely embeds a marker's letters stays exempt: `Form` ⊄ word in
        // `FormatSchema` / `TransformSchema`, `Config` ⊄ word in
        // `ConfigurationSchema`, `Body` ⊄ word in `AntibodySchema`. Without
        // word-anchoring these wire-read schemas would regress to the #513 FP.
        assert!(run("const FormatSchema = z.object({ value: z.string() });").is_empty());
        assert!(run("const TransformSchema = z.object({ value: z.string() });").is_empty());
        assert!(run("const ConfigurationSchema = z.object({ value: z.string() });").is_empty());
        assert!(run("const AntibodySchema = z.object({ value: z.string() });").is_empty());

        // The genuine whole-word marker still flags the input schema.
        assert_eq!(run("const FormSchema = z.object({ value: z.string() });").len(), 1);
        assert_eq!(run("const EditFormSchema = z.object({ value: z.string() });").len(), 1);
    }

    #[test]
    fn input_marker_overrides_response_marker() {
        // Precedence is load-bearing: an input marker wins even when an explicit
        // response marker is also present, because the schema is still a body the
        // user sends. `ConfigResponseSchema` is config input despite `Response`.
        assert_eq!(
            run("const ConfigResponseSchema = z.object({ host: z.string() });").len(),
            1
        );
    }

    #[test]
    fn still_flags_bare_string_in_input_schema() {
        // Ensure the response-schema exemption does not apply to input schemas.
        // camelCase ad-hoc form bodies are not the entity-mirror convention.
        assert_eq!(run("const loginSchema = z.object({ username: z.string() });").len(), 1);
        assert_eq!(run("const CreateUserInput = z.object({ name: z.string() });").len(), 1);
        // An input marker overrides the trailing `Schema`: `<Entity>InputSchema`
        // / `Edit<Entity>InputSchema` are POST/PATCH bodies, still flagged.
        assert_eq!(
            run("const TeamCentralCodeInputSchema = z.object({ code: z.string() });").len(),
            1
        );
        assert_eq!(
            run("const EditTeamCentralCodeInputSchema = z.object({ code: z.string() });").len(),
            1
        );
        // URL query / filter inputs end in `Schema` but are user-controlled.
        assert_eq!(
            run("const ListProductsQuerySchema = z.object({ search: z.string() });").len(),
            1
        );
        assert_eq!(
            run("const ProductFiltersSchema = z.object({ name: z.string() });").len(),
            1
        );
        // Boot-time config validation is input, not a wire-read row.
        assert_eq!(
            run("const EmailConfigSchema = z.object({ host: z.string() });").len(),
            1
        );
    }
}
