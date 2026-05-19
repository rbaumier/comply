//! drizzle-fk-needs-index — tests for the OXC backend.
//!
//! Production traversal lives in `oxc_typescript.rs`. This module exists
//! solely to host the regression suite under the rule's own `tests`
//! cargo target.

use super::oxc_typescript::Check;
use crate::diagnostic::Diagnostic;
use crate::rules::test_helpers::run_oxc_ts;

fn run_on(source: &str) -> Vec<Diagnostic> {
    run_oxc_ts(source, &Check)
}

#[test]
fn flags_fk_without_any_index() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
        });
    "#;
    assert_eq!(run_on(src).len(), 1);
}

#[test]
fn allows_fk_with_inline_chain_index() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
        }, (t) => [
            index("idx_foo_user_id").on(t.userId),
        ]);
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn allows_fk_with_unique_index_in_extras() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
        }, (t) => [
            uniqueIndex("uniq_foo_user").on(t.userId),
        ]);
    "#;
    assert!(run_on(src).is_empty());
}

#[test]
fn allows_object_form_extras() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
        }, (t) => ({
            userIdx: index("idx_foo_user_id").on(t.userId),
        }));
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn allows_composite_pk_leading_fk_column() {
    let src = r#"
        export const teamNetwork = pgTable("team_network", {
            teamId: uuid().notNull().references(() => team.id),
            networkId: uuid().notNull().references(() => network.id),
        }, (t) => [
            primaryKey({ columns: [t.teamId, t.networkId] }),
            index("idx_team_network_network_id").on(t.networkId),
        ]);
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn flags_trailing_pk_fk_column_without_other_index() {
    // Only the LEADING composite-PK column gets a Postgres-side index.
    // A FK that sits in a trailing position still needs its own index.
    let src = r#"
        export const teamNetwork = pgTable("team_network", {
            teamId: uuid().notNull().references(() => team.id),
            networkId: uuid().notNull().references(() => network.id),
        }, (t) => [
            primaryKey({ columns: [t.teamId, t.networkId] }),
        ]);
    "#;
    let diags = run_on(src);
    assert_eq!(diags.len(), 1, "{diags:?}");
}

#[test]
fn flags_when_extras_index_covers_different_column() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
            tenantId: uuid().notNull().references(() => tenants.id),
        }, (t) => [
            index("idx_foo_user").on(t.userId),
        ]);
    "#;
    let diags = run_on(src);
    assert_eq!(diags.len(), 1, "{diags:?}");
}

#[test]
fn reproducer_extras_array_two_fks() {
    let src = r#"
        export const teamCentralCode = pgTable(
            "team_central_code",
            {
                id: uuid().primaryKey(),
                teamId: uuid()
                    .notNull()
                    .references(() => team.id),
                centraleId: uuid()
                    .notNull()
                    .references(() => centrale.id),
            },
            (t) => [
                index("idx_team_central_code_team_id").on(t.teamId),
                index("idx_team_central_code_centrale_id").on(t.centraleId),
            ],
        );
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn reproducer_composite_pk_plus_separate_index() {
    let src = r#"
        export const teamNetwork = pgTable(
            "team_network",
            {
                teamId: uuid().notNull().references(() => team.id),
                networkId: uuid().notNull().references(() => network.id),
            },
            (t) => [
                primaryKey({ columns: [t.teamId, t.networkId] }),
                index("idx_team_network_network_id").on(t.networkId),
            ],
        );
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn ignores_unrelated_call_expressions() {
    let src = r#"
        const x = something("foo", {
            userId: integer().references(() => users.id),
        });
    "#;
    assert!(run_on(src).is_empty());
}

#[test]
fn handles_block_body_extras_callback() {
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            userId: uuid().notNull().references(() => users.id),
        }, (t) => {
            return [index("idx_foo_user").on(t.userId)];
        });
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn allows_fk_with_chained_index_where_clause() {
    // Regression: index("x").on(t.teamId).where(sql`...`) — the top-level
    // callee is `.where(...)`, not `.on(...)`. The chain walker must peel
    // `.where` and find `.on` underneath.
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            teamId: uuid().notNull().references(() => team.id),
        }, (t) => [
            index("idx_foo_team_id").on(t.teamId).where(sql`teamId IS NOT NULL`),
        ]);
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}

#[test]
fn does_not_flag_references_in_string_literal() {
    // Regression: a column comment containing ".references(" text must not
    // be mistaken for an actual FK declaration.
    let src = r#"
        export const foo = pgTable("foo", {
            id: uuid().primaryKey(),
            note: varchar({ comment: "see .references() docs" }).notNull(),
        });
    "#;
    assert!(run_on(src).is_empty(), "{:?}", run_on(src));
}
