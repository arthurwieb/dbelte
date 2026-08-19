<div align="center">

<img src="static/logo.png" alt="dbelte" width="120" />

# dbelte

**A lightweight desktop database manager for PostgreSQL and SQLite.**

Browse and edit table data, run and save SQL, export results — in a minimal orange-on-black UI that starts instantly and stays out of your way.

[![Ko-fi](https://img.shields.io/badge/support-ko--fi-ff3e00?style=flat-square&logo=ko-fi&logoColor=white)](https://ko-fi.com/arthurwieb)

</div>

---

## Why this exists

Most database GUIs are either a browser tab pretending to be an app, or a 400 MB Electron install that takes ten seconds to show you a table. dbelte is a ~10 MB native binary that opens a connection and shows you rows.

The design has three rules:

**The webview never touches your database.** Every query runs in Rust. The frontend calls `invoke()` and gets JSON back. Connection strings, passwords, and driver handles never exist in JavaScript.

**Your passwords aren't ours to keep.** Credentials go into the OS keyring — Secret Service on Linux, Keychain on macOS, Credential Manager on Windows. The app's own metadata store holds everything *except* the password.

**Dynamic SQL is a boundary, not a convenience.** Filters, sorts, and pagination build real SQL, so that builder is the one piece of the codebase treated as hostile territory: every identifier is validated against the live table schema before it's quoted, and every value is a bound parameter. It has unit tests, and they're the tests that matter most.

## Install

> **Not released yet.** Once the first version is published, downloads will
> appear on the [Releases page](../../releases/latest). The steps below are
> what you'll do when it's there.

You don't need to install Rust, bun, or anything else to *use* dbelte — those
are only for building it yourself. Grab the file for your system and run it.

Go to the [Releases page](../../releases/latest) and open the **Assets** list.
You'll see several files. Pick the one that matches your computer:

### Windows

1. Download the file ending in **`.msi`** (for example `dbelte_0.1.0_x64_en-US.msi`).
2. Double-click it.
3. Windows will likely show a blue box: **"Windows protected your PC"**.
   This does *not* mean the file is broken or dangerous — it means the app
   isn't signed with a paid Microsoft certificate, which every small project
   starts out without. Click **More info**, then **Run anyway**.
4. Follow the installer. dbelte then appears in your Start menu.

If `.msi` gives you trouble, the `.exe` in the same list is an alternative
installer that does the same job.

### macOS

1. Download the file ending in **`.dmg`** (for example `dbelte_0.1.0_universal.dmg`).
   The `universal` build works on both Apple Silicon (M1/M2/M3/M4) and older
   Intel Macs, so there's only one to choose from.
2. Double-click it, then drag the **dbelte** icon onto the **Applications**
   folder shown next to it.
3. Open **Applications** and try to launch dbelte. macOS will refuse the first
   time, saying the app is **damaged** or from an **unidentified developer**.
   It isn't damaged — that's macOS's wording for "not signed with a paid Apple
   certificate."
4. To get past it: **right-click** (or Control-click) the dbelte icon and
   choose **Open**, then click **Open** in the dialog. You only do this once;
   afterwards it launches normally.

If right-click → Open still refuses, open the **Terminal** app and run:

```sh
xattr -cr /Applications/dbelte.app
```

That clears the "downloaded from the internet" flag. Then launch it normally.

### Linux

Three formats — pick the one for your distribution:

**AppImage** (works on any distro, no installation, good if you're unsure):

```sh
chmod +x dbelte_0.1.0_amd64.AppImage   # make it runnable, once
./dbelte_0.1.0_amd64.AppImage          # run it
```

**Debian / Ubuntu / Linux Mint / Pop!_OS** — download the `.deb` and:

```sh
sudo apt install ./dbelte_0.1.0_amd64.deb
```

**Fedora / RHEL / openSUSE** — download the `.rpm` and:

```sh
sudo dnf install ./dbelte-0.1.0-1.x86_64.rpm
```

With `.deb` or `.rpm`, dbelte shows up in your applications menu like any
other program. Replace the version numbers above with whatever the files are
actually called.

### First run

Open dbelte and click **New connection**.

- **SQLite** — pick **SQLite** as the engine and click **Browse** to find your
  `.db` file. Nothing else to fill in.
- **PostgreSQL** — if you have a connection URL (it starts with
  `postgres://`), paste it into the top field and the rest of the form fills
  itself in. Otherwise type the host, port, database, username and password by
  hand.

Click **Test** to check it works, then **Save**. Your new connection appears as
a card — click it to start browsing.

Passwords are stored in your operating system's own password manager
(Keychain, Credential Manager, or your Linux keyring), not in a file
belonging to dbelte.

## Concepts

### Connections

A connection is a saved database — name, engine, and how to reach it. PostgreSQL takes host/port/database/user; SQLite takes a file path (use **Browse**, or type an absolute path — a relative one resolves against the app's working directory, not yours).

Paste a `postgres://user:pass@host:5432/db` URL into the top field and the form fills itself in.

Clicking a connection card opens a **workspace**. Pools are opened on entry and closed when you leave.

### The workspace

A sidebar of saved queries and tables, and three tabs for whatever table you've selected. Drag the sidebar's right edge to resize it; the width is remembered.

**Data** — the rows, paginated. Sort by clicking a column header. Double-click a cell to edit it in place. The row limit (50–1000) is a dropdown in the toolbar, because "SELECT everything" is how you hang a client on a big table.

Filters stack with AND and cover more than equality: `contains` / `starts with` / `ends with` build the LIKE pattern for you and escape any `%` or `_` you typed literally, while raw `LIKE` / `ILIKE` leave your wildcards alone. `IN` takes a comma-separated list. **`ILIKE` is PostgreSQL-only** — on SQLite it degrades to `LIKE`, which is already case-insensitive there for ASCII.

**Structure** — the column list, and adding a column. The type field is a searchable dropdown of that engine's real types (SQLite gets its five storage classes, not Postgres's fifty); anything you type that isn't in the list is offered verbatim, so `numeric(12,4)` works.

**Query** — a CodeMirror editor with autocomplete that knows your actual tables and columns. `⌘⏎` / `Ctrl⏎` runs. Save a query and it lands in the sidebar.

### Editing requires a single-column primary key

To update or delete a row, the app has to be able to name that row unambiguously. It uses the primary key. If a table has a composite PK or none at all, it's read-only — the UI hides the controls and the backend refuses anyway. This is a deliberate limit, not a missing feature: guessing at row identity is how a data manager corrupts data.

### Export

CSV or JSON, from either the Data tab or a query result. Export re-runs the query rather than dumping what's on screen, so you get the whole result and not just the current page. Data-tab export currently ignores active filters.

## Support

dbelte is free and always will be. If it saved you from another Electron install, you can [buy me a coffee on Ko-fi](https://ko-fi.com/arthurwieb).

## Stack

| Layer | Tech |
|-------|------|
| Desktop shell | Tauri 2 (custom title bar, no native decorations) |
| Frontend | SvelteKit (Svelte 5 runes, `adapter-static`, SSR off), TypeScript |
| Styling | Tailwind CSS v4 + shadcn-svelte (dark-only, square corners, Svelte orange `#ff3e00`) |
| SQL editor | CodeMirror 6 + `@codemirror/lang-sql` (schema-aware autocomplete) |
| DB access | Rust: `sqlx` (postgres + sqlite, rustls) |
| Secrets | OS keyring (`keyring` crate) |
| App metadata | SQLite file in Tauri's app-data dir (`meta.db`) |
| Package manager | **bun** (never npm) |

## Development

*Only needed if you want to build dbelte yourself or contribute — to just use
it, see [Install](#install) above.*

Prerequisites: [bun](https://bun.sh), [Rust](https://rustup.rs) (stable), and the platform webview toolchain.

Linux (Fedora):

```sh
sudo dnf install webkit2gtk4.1-devel gtk3-devel dbus-devel librsvg2-devel openssl-devel libappindicator-gtk3-devel
```

Linux (Debian/Ubuntu):

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libssl-dev build-essential curl wget file
```

macOS needs Xcode Command Line Tools; Windows needs the MSVC build tools and WebView2 (preinstalled on Windows 11).

Then:

```sh
bun install
bun run tauri dev      # launches the desktop app with HMR
```

First run compiles the whole Rust dependency tree — expect a few minutes; later runs are incremental.

> `bun run dev` alone only serves the frontend in a browser. Every database call goes through Tauri `invoke()`, so nothing works outside the desktop window. Use it only for pure UI/CSS work.

Other useful commands:

```sh
bun run check                      # svelte-check (TS errors)
bun run build                      # frontend production build only (outputs to build/)
cd src-tauri && cargo test --lib   # query-builder unit tests
cd src-tauri && cargo check        # also validates capability permissions
```

A sample database (`sample.db`, tables `users` + `orders`) sits in the repo root for testing.

Replacing the logo: drop a square transparent PNG at `static/logo.png`, then `bunx tauri icon static/logo.png` to regenerate every platform icon.

## Building an executable

```sh
bun run tauri build
```

Runs `bun run build`, compiles Rust in release mode, then packages installers. First release build is slow (5–15 min).

| What | Where |
|------|-------|
| Raw binary | `src-tauri/target/release/dbelte` (`.exe` on Windows) |
| Installers/bundles | `src-tauri/target/release/bundle/` |

`tauri.conf.json` sets `"targets": "all"`, so each platform builds everything it can:

- **Linux** — `bundle/deb/`, `bundle/rpm/`, `bundle/appimage/`. The AppImage is the portable one: `chmod +x` and run.
- **macOS** — `bundle/macos/dbelte.app` and `bundle/dmg/`. Unsigned builds get Gatekeeper-blocked elsewhere; needs an Apple Developer ID to sign and notarize.
- **Windows** — `bundle/nsis/` and `bundle/msi/`.

Single target instead of all:

```sh
bun run tauri build --bundles appimage    # or deb, rpm, dmg, nsis, msi
bun run tauri build --no-bundle           # binary only, skip packaging
```

Cross-compiling is not supported — build each OS on that OS (or in CI).

### Releasing

`.github/workflows/release.yml` builds macOS (universal), Linux and Windows in
parallel and attaches the installers to a **draft** GitHub release.

```sh
# 1. bump the version in all three manifests (they must agree)
#    package.json · src-tauri/tauri.conf.json · src-tauri/Cargo.toml
# 2. tag and push
git tag v0.2.0
git push origin v0.2.0
# 3. review the draft release on GitHub, then publish
```

`tauri-action` reads the version from `tauri.conf.json`, so a tag that disagrees
with it produces confusingly-named artifacts. Trigger the workflow manually
(Actions → Release → Run workflow) the first time to prove the build works
before you commit to a tag.

macOS artifacts are unsigned — other people will hit a Gatekeeper warning until
the workflow is given an Apple Developer ID (`APPLE_CERTIFICATE`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` secrets).

## Architecture

All database work happens in Rust; the webview only calls `invoke()`. Credentials never enter the frontend.

```
src/                            SvelteKit frontend
  lib/api.ts                    typed invoke() wrappers — the full command surface
  lib/confirm.svelte.ts         promise-based confirm() backed by a themed modal
  lib/links.ts                  external URLs (Ko-fi)
  lib/components/
    Grid.svelte                 shared data grid (sort, dbl-click inline edit, row delete)
    DataTab.svelte              filters, row limit, pagination, insert/edit/delete, export
    StructureTab.svelte         column list + ALTER TABLE ADD COLUMN (searchable type picker)
    QueryTab.svelte             CodeMirror editor, run/save/export
    SqlEditor.svelte            CodeMirror 6 setup, theme, schema autocomplete
    TitleBar.svelte             custom window chrome (decorations are off)
    ResizeGrips.svelte          edge/corner resize handles an undecorated window loses
    ConfirmDialog.svelte        the single mounted confirm modal
    Spinner.svelte              the one loading indicator
    ui/                         shadcn-svelte components (copied in, editable)
  routes/
    +layout.svelte              title bar + confirm modal + toaster shell
    +page.svelte                connection cards + add/edit dialog (URL paste autofill)
    c/[id]/+page.svelte         workspace: resizable sidebar + tabs
    layout.css                  the entire theme (shadcn CSS variables, dark-only)

src-tauri/src/                  Rust backend
  lib.rs                        Tauri builder, state, command registration
  meta.rs                       app metadata store (connections, saved queries) + keyring
  db.rs                         Pool enum (Pg|Sqlite), row→JSON decoding,
                                SELECT builder (THE injection boundary — unit tested)
  commands.rs                   all #[tauri::command] handlers
```

Key invariants:

- **Identifiers are never interpolated raw.** Filter/sort columns are validated against the live table schema; identifiers go through `quote_ident`; values are always bound parameters. `db.rs` unit tests cover this — keep them green.
- **Editing requires exactly one PK column.** Tables without a single-column primary key are read-only in the UI (backend enforces too).
- **Passwords live only in the OS keyring** (service `"dbelte"`, key = connection UUID). `meta.db` holds everything else.
- **Window controls need explicit capabilities.** `core:window:default` is read-only; minimize/close/toggle-maximize/dragging are granted one by one in `src-tauri/capabilities/`. A missing one fails silently at runtime — `cargo check` validates the identifiers.
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set on Linux in `lib.rs` — WebKitGTK's DMA-BUF renderer segfaults on some GPU stacks (AMD/Wayland included). Don't remove it.
- keyring uses the `sync-secret-service` feature on Linux. The async zbus backend panics inside Tauri's tokio runtime ("cannot start a runtime from within a runtime").

## Adding a database engine

The architecture is shaped for this: `DbPool` is an enum, so adding an arm makes the compiler list every place that needs a decision.

**MySQL/MariaDB** is roughly half a day — sqlx has a native driver, so the whole query/bind/execute path generalizes. New enum arm, an `open` branch, a `mysql_value` decoder mirroring `pg_value`, and two `information_schema` introspection queries. The one careful bit is `quote_ident`, which hardcodes `"` and would need backticks. Worth converting `build_select`'s `is_pg: bool` into a `Dialect` enum first — it already carries two meanings (placeholder style *and* ILIKE support).

**Anything sqlx doesn't ship** (MSSQL, Oracle, DuckDB) is days, not hours: different driver APIs mean `execute` no longer generalizes, and dialects diverge further (`OFFSET…FETCH`, `[brackets]`, `@p1`).

**MongoDB and friends** don't fit at all — `build_select`, `ColumnInfo`, and "exactly one PK column" have no analogue. That would be a separate view, not a `DbPool` arm.

## Current limitations

Deliberate cuts, roughly in order of how likely you are to hit them:

- PostgreSQL introspection covers the `public` schema only.
- Composite primary keys → table is read-only.
- Data-tab export ignores active filters (it re-runs the raw table query).
- No total row count, so pagination shows "page 2" and not "page 2 of 14".
- Filter values bind by declared column type — exotic types (arrays, enums, ranges) fall back to text comparison.
- `sslmode` / `channel_binding` URL params are ignored (sqlx defaults to TLS-preferred, which Neon accepts).
- No query cancellation — a long query blocks its tab.
- Bad hosts spin until sqlx gives up; no connection timeout is surfaced.
- `add_column` whitelists type strings to alphanumerics plus `(),_ `, so array types like `text[]` are rejected.

## Gotchas learned the hard way

- `shadcn-svelte init` is interactive and fights automation — `components.json` was written by hand; `shadcn-svelte add -y <components>` works fine after that. `add -o` will overwrite existing components, so back up `src/lib/components/ui/` first.
- Svelte 5: don't read reactive state inside the `$effect` that creates CodeMirror — every keystroke recreates the editor and drops focus. It's created once in `onMount` with `untrack`.
- Tauri v2 auto-converts command arg names: Rust `snake_case` ⇄ JS `camelCase`. `default` is a Rust keyword — the add-column arg is `default_value`.
- `sqlx` with `default-features = false` needs the `macros` feature for `#[derive(FromRow)]`.
- An undecorated window loses the WM's resize border — hence `ResizeGrips.svelte`. Without it the window is stuck at its initial size on Linux.
- External links must go through `@tauri-apps/plugin-opener`; a plain `<a href>` navigates the webview away from the app.
