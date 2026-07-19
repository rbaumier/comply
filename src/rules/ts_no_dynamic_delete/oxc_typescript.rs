//! OXC backend for ts-no-dynamic-delete.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_process_env(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else { return false };
    if member.property.name.as_str() != "env" {
        return false;
    }
    let Expression::Identifier(obj) = &member.object else { return false };
    obj.name.as_str() == "process"
}

/// Name of the rightmost identifier/property of a member-access chain or identifier.
/// `require` -> "require"; `ctx.nativeRequire` -> "nativeRequire"; `module.constructor` -> "constructor".
fn tail_name<'a>(expr: &'a Expression) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

/// Allow `delete require.cache[id]` / `delete Module._cache[id]` and their aliases.
///
/// The Node module cache is a `Record<string, NodeModule>` keyed by resolved module path,
/// so a computed-key delete is the canonical cache-busting idiom (not a fixed-shape object).
/// Receivers: `require.cache` / `nativeRequire.cache` (incl. `ctx.nativeRequire.cache`),
/// `Module._cache`, `module.constructor._cache`.
fn is_module_cache(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else { return false };
    match member.property.name.as_str() {
        "cache" => matches!(tail_name(&member.object), Some("require" | "nativeRequire")),
        "_cache" => matches!(tail_name(&member.object), Some("Module" | "constructor")),
        _ => false,
    }
}

/// Structural identity of an object expression used as a dictionary base.
///
/// Only the shapes the polyfill idiom actually uses are comparable: a bare
/// identifier (`headers` / `env`) and a `this.<prop>` member (`this._events` /
/// `this._headers`). Anything else is `None` and never matches, so arbitrary
/// member chains on foreign objects can't leak an exemption.
#[derive(PartialEq)]
enum DictBase<'a> {
    Ident(&'a str),
    ThisMember(&'a str),
}

fn dict_base<'a>(expr: &Expression<'a>) -> Option<DictBase<'a>> {
    match expr {
        Expression::Identifier(ident) => Some(DictBase::Ident(ident.name.as_str())),
        Expression::StaticMemberExpression(member)
            if matches!(member.object, Expression::ThisExpression(_)) =>
        {
            Some(DictBase::ThisMember(member.property.name.as_str()))
        }
        _ => None,
    }
}

/// Base of a computed-key assignment target whose key is *dynamic* (not a string
/// or number literal). A literal key (`obj["a"] = …`) is equivalent to a fixed
/// property and doesn't prove dictionary usage, so it's excluded.
fn assignment_target_base<'a>(target: &AssignmentTarget<'a>) -> Option<DictBase<'a>> {
    match target {
        AssignmentTarget::ComputedMemberExpression(member)
            if !matches!(
                &member.expression,
                Expression::StringLiteral(_) | Expression::NumericLiteral(_)
            ) =>
        {
            dict_base(&member.object)
        }
        _ => None,
    }
}

/// True when the same object is also written through a dynamic computed key
/// (`base[expr] = …` or a compound assignment) somewhere in the file.
///
/// A dynamic computed-key write proves the object is used as a dynamic dictionary
/// (string-indexed map), so a computed-key delete is the correct idiom rather
/// than a dynamic delete on a fixed-shape object — matching typescript-eslint,
/// which doesn't flag index-signature / `Record` targets.
fn is_written_as_dictionary<'a>(
    delete_base: &Expression,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(base) = dict_base(delete_base) else {
        return false;
    };
    semantic.nodes().iter().any(|n| {
        let AstKind::AssignmentExpression(assign) = n.kind() else {
            return false;
        };
        assignment_target_base(&assign.left).is_some_and(|written| written == base)
    })
}

/// Symbol an identifier reference resolves to, or `None` for an unresolved
/// reference (a global or an unbound name). Used to compare two occurrences of a
/// name by *binding* rather than by spelling, so a rebound/shadowed name can't be
/// mistaken for the original.
fn reference_symbol(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::SymbolId> {
    let ref_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
}

/// True when `receiver` and `iterand` denote the same object: the same resolved
/// binding for a bare identifier, or the same base binding followed by an
/// identical static-property chain for a member access (`obj.nested`). Computed
/// or non-identifier-rooted shapes return `false`, so only structurally-equal
/// receivers bound to the same symbol match.
fn same_object(
    receiver: &Expression,
    iterand: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match (receiver, iterand) {
        (Expression::Identifier(a), Expression::Identifier(b)) => {
            match (reference_symbol(a, semantic), reference_symbol(b, semantic)) {
                (Some(sa), Some(sb)) => sa == sb,
                _ => false,
            }
        }
        (Expression::StaticMemberExpression(a), Expression::StaticMemberExpression(b)) => {
            a.property.name.as_str() == b.property.name.as_str()
                && same_object(&a.object, &b.object, semantic)
        }
        _ => false,
    }
}

/// True when the `delete obj[k]` at `delete_id` removes the current key of an
/// enclosing `for (const k in obj)` enumeration — the receiver `obj` is the same
/// object as the loop iterand and the deleted key `k` is the loop's binding.
///
/// `for…in` enumerates an object's own enumerable string keys, so iterating one is
/// itself evidence the object is used as a dynamic string-keyed map rather than a
/// fixed-shape interface; pruning an enumerated key is the canonical
/// remove-during-enumeration idiom, not a dynamic delete on a fixed-shape object.
///
/// Both halves are matched by resolved binding, never by spelling: the key must
/// resolve to the loop's binding symbol, and the receiver must resolve to the
/// same symbol(s) as the iterand. The ancestor walk stops at the first function
/// boundary, so a delete inside a closure nested in the loop body — where `k` is a
/// different binding — does not match the outer loop.
fn is_for_in_enumeration_delete(
    delete_id: oxc_semantic::NodeId,
    receiver: &Expression,
    key: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{BindingPattern, ForStatementLeft};

    let Expression::Identifier(key_ident) = key else {
        return false;
    };
    let Some(key_symbol) = reference_symbol(key_ident, semantic) else {
        return false;
    };

    for kind in semantic.nodes().ancestor_kinds(delete_id) {
        // A for-in binding is in scope only up to the enclosing function; a
        // nested closure introduces its own `k`, so stop the walk there.
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            return false;
        }
        let AstKind::ForInStatement(stmt) = kind else {
            continue;
        };
        let ForStatementLeft::VariableDeclaration(decl) = &stmt.left else {
            continue;
        };
        // A for-in head declares exactly one binding.
        let Some(BindingPattern::BindingIdentifier(bound)) =
            decl.declarations.first().map(|d| &d.id)
        else {
            continue;
        };
        if bound.symbol_id.get() == Some(key_symbol)
            && same_object(receiver, &stmt.right, semantic)
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else { return };
        if unary.operator != UnaryOperator::Delete {
            return;
        }

        // Argument must be a computed member expression: obj[expr]
        let Expression::ComputedMemberExpression(member) = &unary.argument else {
            return;
        };

        // Allow `delete process.env[key]` — only way to unset an env var in Node.js.
        if is_process_env(&member.object) {
            return;
        }

        // Allow `delete require.cache[id]` / `delete Module._cache[id]` — the Node module
        // cache is dictionary-keyed by module path, so a computed-key delete is the
        // canonical cache-busting idiom, not a dynamic delete on a fixed-shape object.
        if is_module_cache(&member.object) {
            return;
        }

        // Allow deletes on an object that is also written through a computed key
        // elsewhere in the file: that write proves dictionary (string-indexed map)
        // usage, so a computed-key delete is the correct idiom. Covers Node.js
        // polyfill stores like EventEmitter `events[type]`, HTTP `this._headers`,
        // and the `process.env` proxy's `env[prop]`.
        if is_written_as_dictionary(&member.object, semantic) {
            return;
        }

        // Allow `delete obj[k]` that prunes the current key of an enclosing
        // `for (const k in obj)` loop: enumerating an object treats it as a
        // dynamic string-keyed map, so removing an enumerated key is the
        // canonical remove-during-enumeration idiom, not a fixed-shape delete.
        if is_for_in_enumeration_delete(node.id(), &member.object, &member.expression, semantic) {
            return;
        }

        // Allow literal string/number keys.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            // Allow negative number literals: `-42`
            Expression::UnaryExpression(inner)
                if inner.operator == UnaryOperator::UnaryNegation
                    && matches!(&inner.argument, Expression::NumericLiteral(_)) =>
            {
                return;
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.expression.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not delete dynamically computed property keys — use `Map` or `Set`."
                .into(),
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_static_string_delete() {
        assert!(run_on(r#"delete obj["foo"];"#).is_empty());
    }

    #[test]
    fn allows_static_number_delete() {
        assert!(run_on("delete obj[42];").is_empty());
    }

    #[test]
    fn allows_dot_property_delete() {
        assert!(run_on("delete obj.foo;").is_empty());
    }

    // Regression #558 — process.env teardown in tests
    #[test]
    fn allows_delete_process_env_dynamic_key() {
        assert!(run_on("delete process.env[key];").is_empty());
    }

    #[test]
    fn allows_delete_process_env_string_literal_key() {
        assert!(run_on(r#"delete process.env['MY_VAR'];"#).is_empty());
    }

    #[test]
    fn still_flags_non_process_env_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    // Regression #5252 — Node module cache busting in a module loader (jiti)
    #[test]
    fn allows_delete_require_cache_dynamic_key() {
        assert!(run_on("delete require.cache[id];").is_empty());
    }

    #[test]
    fn allows_delete_native_require_cache_member_chain() {
        assert!(run_on("delete ctx.nativeRequire.cache[id];").is_empty());
    }

    #[test]
    fn allows_delete_module_cache_dynamic_key() {
        assert!(run_on("delete Module._cache[resolved];").is_empty());
    }

    #[test]
    fn allows_delete_module_constructor_cache() {
        assert!(run_on("delete module.constructor._cache[resolved];").is_empty());
    }

    #[test]
    fn still_flags_unrelated_dot_cache_dynamic_delete() {
        // A plain `.cache` whose base is not require/nativeRequire is an ordinary object.
        let diags = run_on("delete obj.cache[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_process_env_teardown_pattern() {
        let src = r#"
const backup: Record<string, string | undefined> = {};
beforeEach(() => {
  backup['MY_VAR'] = process.env['MY_VAR'];
  delete process.env['MY_VAR'];
});
afterEach(() => {
  if (backup['MY_VAR'] === undefined) {
    delete process.env['MY_VAR'];
  } else {
    process.env['MY_VAR'] = backup['MY_VAR'];
  }
});
"#;
        assert!(run_on(src).is_empty());
    }

    // Regression #5253 — Node.js polyfill dictionary stores. Each object is also
    // written through a computed key in the same file, proving dictionary usage.

    #[test]
    fn allows_delete_event_emitter_events_store() {
        let src = r#"
function removeListener(events, type, list) {
  events[type] = undefined;
  delete events[type];
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_delete_http_headers_this_member() {
        let src = r#"
class OutgoingMessage {
  setHeader(name: string, value: string): void {
    this._headers[name.toLowerCase()] = value;
  }
  removeHeader(name: string): void {
    delete this._headers[name.toLowerCase()];
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_delete_process_env_proxy_trap() {
        let src = r#"
new Proxy(_envShim, {
  set(_, prop, value) {
    const env = _getEnv(true);
    env[prop as string] = value;
    return true;
  },
  deleteProperty(_, prop) {
    const env = _getEnv(true);
    delete env[prop as string];
    return true;
  },
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_dynamic_delete_without_computed_write() {
        // `config` only has static property access — never written by computed key,
        // so the delete is a genuine dynamic delete on a fixed-shape object.
        let src = r#"
function reset(config, userKey) {
  config.enabled = true;
  delete config[userKey];
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_dynamic_delete_when_only_static_key_written() {
        // A static-key write (`config["a"] = …`) does not prove dictionary usage.
        let src = r#"
function reset(config, userKey) {
  config["a"] = 1;
  delete config[userKey];
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    // Regression #5539 — mongoose ODM projection / schema-path maps.

    #[test]
    fn allows_mongoose_query_select_projection_maps() {
        let src = r#"
function select(arg, fields, userProvidedFields) {
  Object.entries(arg).forEach(([key, value]) => {
    if (value) {
      if (fields['-' + key] != null) {
        delete fields['-' + key];
      }
      fields[key] = userProvidedFields[key] = sanitizeValue(value);
    } else {
      Object.keys(userProvidedFields).forEach(field => {
        if (isSubpath(key, field)) {
          delete fields[field];
          delete userProvidedFields[field];
        }
      });
    }
  });
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 0, "{diags:?}");
    }

    #[test]
    fn allows_mongoose_schema_nested_path_for_in_delete() {
        let src = r#"
function applySchema(newSchema, paths) {
  for (const nested in newSchema.singleNestedPaths) {
    if (paths.includes(nested)) {
      delete newSchema.singleNestedPaths[nested];
    }
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 0, "{diags:?}");
    }

    #[test]
    fn allows_for_in_enumeration_delete_bare_receiver() {
        let src = r#"
function prune(map) {
  for (const k in map) {
    delete map[k];
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 0, "{diags:?}");
    }

    #[test]
    fn still_flags_delete_of_non_loop_key_inside_for_in() {
        // The deleted key is not the loop variable, so this is a genuine dynamic
        // delete on a possibly fixed-shape object, not enumeration pruning.
        let src = r#"
function prune(obj, other) {
  for (const k in obj) {
    delete obj[other];
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_delete_of_different_receiver_inside_for_in() {
        // The delete receiver is not the iterand; enumerating `keys` says nothing
        // about `config` being a dictionary.
        let src = r#"
function prune(keys, config) {
  for (const k in keys) {
    delete config[k];
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_delete_when_receiver_rebound_inside_for_in() {
        // `obj` is shadowed by a new binding before the delete, so the deleted
        // receiver is a different object than the loop iterand even though the
        // name matches — symbol resolution keeps this flagged.
        let src = r#"
function prune(obj, other) {
  for (const k in obj) {
    const obj = other;
    delete obj[k];
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn still_flags_delete_in_nested_closure_with_shadowed_key() {
        // The delete sits in a closure nested in the loop body; its `k` is a
        // distinct parameter, not the enumeration key, so the delete must flag.
        let src = r#"
function prune(map) {
  for (const k in map) {
    [1].forEach((k) => {
      delete map[k];
    });
  }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }
}
