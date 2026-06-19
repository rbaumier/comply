//! zod-patch-schema-require-non-empty OXC backend.
//!
//! Fires on `const <Edit|Patch|Update...Schema> = z.object({...})` (or a
//! `.partial()` result) where every field is `.optional()`/`.nullable()`/
//! `.nullish()` and no `.refine`/`.superRefine`/`.check` guards non-emptiness.
//! Such a schema validates `{}` → a silent no-op update.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use regex::Regex;
use std::sync::{Arc, LazyLock};

pub struct Check;

/// PATCH-intent binding names: `Edit`/`Patch`/`Update` as a camelCase/PascalCase
/// segment (followed by an uppercase letter or end-of-word, so `Updated` /
/// `Editor` do not match), somewhere before `Schema` (e.g. `UserEditSchema`,
/// `updatePostSchema`). `lastUpdatedSchema` / `editorSchema` are excluded.
static PATCH_SCHEMA_NAME: LazyLock<Regex> = LazyLock::new(|| {
    // Trigger word, then zero or more PascalCase segments, then a trailing
    // `Schema`. Requiring `[A-Z]`-led segments after the trigger means a
    // following lowercase letter (`Updated`, `Editor`) cannot match — only a
    // real segment boundary (`UpdateSchema`, `updatePostSchema`) does.
    Regex::new(r"(?:Edit|Patch|Update|edit|patch|update)(?:[A-Z][a-zA-Z]*)*Schema$")
        .expect("static regex compiles")
});

/// Methods that guard non-emptiness — their presence anywhere in the chain
/// exempts the schema (the author opted into a runtime check).
const GUARD_METHODS: &[&str] = &["refine", "superRefine", "check"];

/// Chain methods that can reintroduce required fields the walker cannot see
/// (`.extend({...})`, `.merge(Base)`, `.and(Other)`). If any appears in the
/// chain we cannot prove the schema is all-optional, so we bail out.
const FIELD_ADDING_METHODS: &[&str] = &["extend", "merge", "and"];

/// Terminal optionality wrappers. A property value whose outermost call is one
/// of these is an optional field.
const OPTIONAL_METHODS: &[&str] = &["optional", "nullable", "nullish"];

/// Walk a `.method()` chain from the outermost expression inward, recording
/// whether a non-emptiness guard, an argument-less `.partial()`, or a
/// field-adding method appeared, plus the innermost `z.object({...})` literal
/// (if the chain bottoms out there).
struct ChainInfo<'a> {
    has_guard: bool,
    has_partial: bool,
    has_field_adding: bool,
    object_arg: Option<&'a oxc_ast::ast::ObjectExpression<'a>>,
}

fn walk_chain<'a>(mut expr: &'a Expression<'a>) -> ChainInfo<'a> {
    let mut info = ChainInfo {
        has_guard: false,
        has_partial: false,
        has_field_adding: false,
        object_arg: None,
    };

    loop {
        let Expression::CallExpression(call) = expr else { return info };

        // `z.object({...})` — record the object literal and stop descending.
        if let Expression::StaticMemberExpression(member) = &call.callee
            && member.property.name.as_str() == "object"
            && matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "z")
        {
            if let Some(Expression::ObjectExpression(obj)) =
                call.arguments.first().and_then(|a| a.as_expression())
            {
                info.object_arg = Some(obj);
            }
            return info;
        }

        // Otherwise it must be a `<receiver>.method(...)` link to descend through.
        let Expression::StaticMemberExpression(member) = &call.callee else { return info };
        let method = member.property.name.as_str();
        if GUARD_METHODS.contains(&method) {
            info.has_guard = true;
        }
        if FIELD_ADDING_METHODS.contains(&method) {
            info.has_field_adding = true;
        }
        // Only argument-less `.partial()` makes EVERY field optional. A masked
        // `.partial({ a: true })` leaves the unlisted fields required, so the
        // schema still rejects `{}` — do not treat it as all-optional.
        if method == "partial" && call.arguments.is_empty() {
            info.has_partial = true;
        }
        expr = &member.object;
    }
}

/// True when every property in the object literal is an optional field
/// (outermost call is `.optional()`/`.nullable()`/`.nullish()`), and the
/// literal has at least one property.
fn all_fields_optional(obj: &oxc_ast::ast::ObjectExpression) -> bool {
    if obj.properties.is_empty() {
        return false;
    }
    obj.properties.iter().all(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            // Spread (`...base`) or other property kinds may carry required
            // fields — do not treat the schema as all-optional.
            return false;
        };
        value_is_optional(&p.value)
    })
}

/// True when the outermost call of a property value is an optionality wrapper.
fn value_is_optional(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    OPTIONAL_METHODS.contains(&member.property.name.as_str())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.object"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else { return };

        // Name gate: only PATCH/Edit/Update schemas.
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id else { return };
        if !PATCH_SCHEMA_NAME.is_match(ident.name.as_str()) {
            return;
        }

        let Some(init) = &decl.init else { return };
        let info = walk_chain(init);

        // Must bottom out in a `z.object({...})`.
        let Some(obj) = info.object_arg else { return };

        // A non-emptiness guard anywhere in the chain exempts the schema.
        if info.has_guard {
            return;
        }

        // A `.extend`/`.merge`/`.and` can reintroduce required fields the
        // walker cannot inspect — we cannot prove the schema is all-optional.
        if info.has_field_adding {
            return;
        }

        // All-optional via `.partial()`, or every field already optional.
        if !info.has_partial && !all_fields_optional(obj) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` is an all-optional PATCH schema — it validates an empty `{{}}` body, \
                 silently updating nothing. Add `.refine(o => Object.keys(o).length >= 1, …)` \
                 or require at least one field.",
                ident.name.as_str()
            ),
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

    #[test]
    fn flags_all_optional_edit_schema() {
        let d = run(
            "const UserEditSchema = z.object({ name: z.string().optional(), bio: z.string().optional() });",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("UserEditSchema"));
    }

    #[test]
    fn allows_refine_guard() {
        assert!(
            run("const UserEditSchema = z.object({ name: z.string().optional(), bio: z.string().optional() }).refine(o => Object.keys(o).length >= 1, { error: 'required' });")
                .is_empty()
        );
    }

    #[test]
    fn allows_required_field() {
        assert!(run("const UserUpdateSchema = z.object({ name: z.string() });").is_empty());
    }

    #[test]
    fn allows_mixed_with_one_required() {
        assert!(
            run("const UserPatchSchema = z.object({ name: z.string(), bio: z.string().optional() });")
                .is_empty()
        );
    }

    #[test]
    fn no_fire_on_non_matching_name() {
        // Name gate: filter/query schemas are not PATCH bodies.
        assert!(
            run("const UserFilterSchema = z.object({ q: z.string().optional() });").is_empty()
        );
        assert!(
            run("const SearchSchema = z.object({ q: z.string().optional() });").is_empty()
        );
    }

    #[test]
    fn no_fire_when_trigger_is_a_substring_not_a_segment() {
        // `Updated`/`Editor` contain `Update`/`Edit` but are not PATCH-intent
        // segments — these read-model/timestamp schemas must not fire.
        assert!(
            run("const lastUpdatedSchema = z.object({ at: z.string().optional() });").is_empty()
        );
        assert!(
            run("const updatedAtSchema = z.object({ at: z.string().optional() });").is_empty()
        );
        assert!(
            run("const editorSchema = z.object({ name: z.string().optional() });").is_empty()
        );
    }

    #[test]
    fn flags_partial_edit_schema() {
        let d = run("const PostEditSchema = z.object({ title: z.string(), body: z.string() }).partial();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn no_fire_on_masked_partial() {
        // `.partial({ title: true })` leaves `body` required — `{}` is rejected,
        // so this is not a no-op-update schema.
        assert!(
            run("const PostEditSchema = z.object({ title: z.string(), body: z.string() }).partial({ title: true });")
                .is_empty()
        );
    }

    #[test]
    fn no_fire_when_extend_adds_required_field() {
        // `.extend({ slug: z.string() })` adds a required field after the
        // partial — `{}` is rejected. The walker cannot see inside `.extend`,
        // so we conservatively do not flag.
        assert!(
            run("const PostEditSchema = z.object({ title: z.string() }).partial().extend({ slug: z.string() });")
                .is_empty()
        );
    }

    #[test]
    fn no_fire_when_merge_adds_required_field() {
        // `.merge(Base)` can inject required fields the walker cannot inspect.
        assert!(
            run("const PostEditSchema = z.object({ title: z.string().optional() }).merge(Base);")
                .is_empty()
        );
    }

    #[test]
    fn flags_nullable_and_nullish() {
        assert_eq!(
            run("const ItemUpdateSchema = z.object({ a: z.string().nullable(), b: z.number().nullish() });").len(),
            1
        );
    }

    #[test]
    fn allows_super_refine_guard() {
        assert!(
            run("const UserEditSchema = z.object({ name: z.string().optional() }).superRefine(() => {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_empty_object() {
        // No properties → no all-optional claim, nothing meaningful to flag.
        assert!(run("const UserEditSchema = z.object({});").is_empty());
    }

    #[test]
    fn flags_lowercase_update_schema() {
        let d = run("const updatePostSchema = z.object({ title: z.string().optional() });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn no_fire_on_spread_field() {
        // A spread may inject required fields — not provably all-optional.
        assert!(
            run("const UserEditSchema = z.object({ ...base, name: z.string().optional() });")
                .is_empty()
        );
    }
}
