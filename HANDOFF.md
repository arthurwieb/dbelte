# Session handoff — dbelte

State as of 2026-08-18: v1 works end-to-end. Connections (Postgres + SQLite), data browsing/editing, filters, saved queries, ALTER TABLE add column, CSV/JSON export, schema-aware SQL autocomplete. Tested against the bundled `sample.db` and a Neon Postgres instance.

## Current limitations (deliberate v1 cuts)

- Postgres introspection covers the `public` schema only.
- Composite primary keys → table is read-only.
- Export always re-runs the full query/table; Data-tab export ignores active filters.
- Filter values bind as text/number by declared column type — exotic types (arrays, enums, ranges) fall back to text comparison.
- `sslmode`/`channel_binding` URL params are ignored (sqlx defaults to TLS-preferred, which Neon accepts).
- No connection timeouts surfaced in UI — a bad host just spins until sqlx gives up.

## Future features, roughly in order of value

1. **Multi-schema support (Postgres)** — schema picker in sidebar; qualify identifiers as `"schema"."table"`. Touches `list_tables`, `fetch_schema`, `build_select`, sidebar UI.
2. **Filtered export** — pass the Data tab's filters/sort to `export_rows` instead of raw SQL (reuse `build_select`, drop the `LIMIT`).
3. **Query history** — auto-log every executed query (per connection, timestamped) into `meta.db`; history panel under saved queries. Cheap and very useful.
4. **Multiple query tabs / result tabs** — currently one editor + one result grid.
5. **Composite PK editing** — pass all PK columns in WHERE instead of refusing.
6. **Table DDL beyond add-column** — drop/rename column (SQLite needs table-rebuild dance), create/drop table, create index.
7. **Row count + total pages** — a `SELECT count(*)` alongside `fetch_rows` (respecting filters) so pagination shows "page 2 of 14".
8. **Cancel running query** — long queries currently block the tab; needs a cancel token around sqlx futures.
9. **Connection health indicator** — ping on workspace focus, reconnect button; pools currently die silently if the server drops.
10. **Real Postgres LSP** — Supabase `postgrestools` as a Tauri sidecar binary feeding diagnostics into CodeMirror (via `@codemirror/lint`). Big lift; the current schema autocomplete covers most of the value.
11. **MySQL/MariaDB engine** — third arm on the `DbPool` enum + introspection queries; sqlx supports it natively.
12. **Import** — CSV → table (column mapping dialog).
13. **JSON cell viewer/editor** — pretty-print modal for jsonb columns instead of one-line truncation.
14. **Keyboard palette** — ctrl-k to jump to table/saved query.

## Where to start next session

- `src/lib/api.ts` shows the whole command surface in one file — fastest way back into the mental model.
- Adding a Tauri command: handler in `src-tauri/src/commands.rs`, register in `lib.rs` `generate_handler!`, wrapper in `api.ts`.
- The injection boundary is `db.rs::build_select` + `quote_ident` — any new dynamic SQL must follow the same pattern (validate identifiers against schema, bind values). Tests: `cargo test --lib` in `src-tauri/`.
