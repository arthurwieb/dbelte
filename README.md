<div align="center">

<img src="static/logo.png" alt="dbelte" width="120" />

# dbelte

**A lightweight desktop database manager for PostgreSQL, MySQL, SQL Server and
SQLite.**

Browse and edit table data, run and save SQL, export results. The Linux package is 8.5 MB.

[![Download](https://img.shields.io/github/v/release/arthurwieb/dbelte?style=flat-square&color=ff3e00&label=download)](../../releases/latest)
[![Ko-fi](https://img.shields.io/badge/support-ko--fi-ff3e00?style=flat-square&logo=ko-fi&logoColor=white)](https://ko-fi.com/arthurwieb)

</div>

---

## Screenshots

<div align="center">

<img src="docs/screenshots/connections.png" alt="Connection list" width="820" />

*Your saved databases. Click a card to open it.*

<img src="docs/screenshots/data.png" alt="Browsing a table" width="820" />

*Browse and filter rows, sort by any column, double-click a cell to edit it.*

<img src="docs/screenshots/query.png" alt="SQL editor" width="820" />

*Write SQL with autocomplete that knows your tables and columns. `Ctrl+Enter` runs it.*

</div>

## What it does

- **Browse tables.** Paginated rows, click a header to sort, filters that stack with AND (`contains`, `starts with`, `IN`, `LIKE`).
- **Edit rows.** Double-click a cell. A dialog shows the old value, the new value, and which row the update hits before anything is written.
- **Run SQL.** A CodeMirror editor with autocomplete that reads your real tables and columns, one-key formatting, saved queries, and the last 50 statements per connection.
- **Follow foreign keys.** A value that points at another table gets a **↗**. Click it to land on that table, filtered to that row.
- **Export.** CSV or JSON. Export re-runs the query, so you get the full result and not the page on screen.
- **Read big values.** JSON and long text open in a syntax-coloured dialog instead of a one-line box.
- **Stay small.** The rpm is 8.5 MB and the app is a native binary, so there is no bundled browser.
- **Keep passwords in the OS keyring.** Keychain, Credential Manager, or your Linux keyring. No password lands in a dbelte file.

## Install

You do not need Rust, bun, or anything else to use dbelte. Those are for building
it yourself. Download the file for your system from the
[releases page](../../releases/latest) and run it.

### Windows

1. Download the file ending in `.msi`, for example `dbelte_0.2.0_x64_en-US.msi`.
2. Double-click it.
3. Windows will probably show a blue box saying **Windows protected your PC**.
   The file is not broken or dangerous. The app is not signed with a paid
   Microsoft certificate, which every small project starts out without. Click
   **More info**, then **Run anyway**.
4. Follow the installer. dbelte then appears in your Start menu.

If the `.msi` gives you trouble, the `.exe` in the same list installs the same app.

### macOS

1. Download the file ending in `.dmg`, for example `dbelte_0.2.0_universal.dmg`.
   The universal build runs on Apple Silicon and on older Intel Macs, so there
   is only one file to choose.
2. Double-click it, then drag the dbelte icon onto the Applications folder
   shown next to it.
3. Open Applications and launch dbelte. macOS refuses the first time and says
   the app is damaged or comes from an unidentified developer. Nothing is
   damaged. That is how macOS describes an app signed without a paid Apple
   certificate.
4. Right-click the dbelte icon, choose **Open**, then click **Open** in the
   dialog. You do this once. After that it launches normally.

If right-click then Open still refuses, open Terminal and run:

```sh
xattr -cr /Applications/dbelte.app
```

That clears the "downloaded from the internet" flag. Then launch it normally.

### Linux

Three formats. Pick the one for your distribution.

AppImage runs on any distro and installs nothing. Use it if you are unsure:

```sh
chmod +x dbelte_0.2.0_amd64.AppImage   # make it runnable, once
./dbelte_0.2.0_amd64.AppImage          # run it
```

Debian, Ubuntu, Linux Mint, Pop!_OS. Download the `.deb` and run:

```sh
sudo apt install ./dbelte_0.2.0_amd64.deb
```

Fedora, RHEL, openSUSE. Download the `.rpm` and run:

```sh
sudo dnf install ./dbelte-0.2.0-1.x86_64.rpm
```

The `.deb` and `.rpm` put dbelte in your applications menu like any other
program. Replace the version numbers above with whatever your files are called.

## First run

Open dbelte and click **New connection**.

For SQLite, pick SQLite as the engine and click **Browse** to find your `.db`
file. Nothing else to fill in.

For PostgreSQL, MySQL, MariaDB or SQL Server, paste your connection URL into the
top field. It accepts `postgres://`, `mysql://` and `sqlserver://`, and the rest
of the form fills itself in. Otherwise type the host, port, database, username
and password by hand.

Click **Test** to check it works, then **Save**. Your new connection appears as a
card. Click it to start browsing.

Your operating system's password manager holds the password. dbelte's own files
never do.

## Using dbelte

### Connections

A connection is a saved database. It has a name, an engine, and how to reach it.
PostgreSQL, MySQL and SQL Server take host, port, database and user. SQLite
takes a file path. Use **Browse**, or type an absolute path, because a relative
path resolves against the app's working directory rather than yours.

MySQL has no schemas of its own. The database you connect to *is* the schema, so
its table names are unqualified, the same as SQLite's. PostgreSQL and SQL Server
do have them, and the sidebar hides the default one, `public` or `dbo`, because
it is noise until it isn't.

For a SQL Server named instance, put it in the host field as `HOST\INSTANCE`.
dbelte asks the SQL Browser service for its port. SQL logins only for now, no
Windows or AD authentication. The server's own self-signed certificate is
accepted, since almost nobody replaces it.

SQL Server is the one engine where **Database** may be left blank. Unlike
PostgreSQL it does not need one to connect, and falls back to whatever the login
has as its default, usually `master`. Blank is allowed for that reason, but it
is rarely what you want. Everything dbelte shows is scoped to one database and
there is no switcher, so a blank field means a connection that can only ever see
`master`. Name the database you actually want to browse.

Clicking a connection card opens a workspace.

### The workspace

The sidebar holds saved queries, recently run statements, and tables. Three tabs
show whatever table you select. Drag the sidebar's right edge to resize it and
the width is remembered. Past ten tables, a filter box appears above the list.

On PostgreSQL the list covers every schema you own, not only `public`. Tables
outside `public` show as `schema.table`, and filters, edits, exports and
foreign-key jumps all carry the schema, so two tables with the same name in
different schemas stay distinct.

### Data tab

The rows, paginated. **⟳** re-reads the current page. Click a column header to
sort. Double-click a cell to edit it, then confirm the change in a dialog that
shows the old value, the new value, and which row it lands on. The row limit is
a dropdown in the toolbar and runs from 50 to 1000, because selecting everything
is how you hang a client on a big table. The pager counts the filtered set, so
it reads "page 2 of 14 · 200 rows of 2731".

A cell holding JSON, or text longer than a line, opens in a dialog rather than an
inline box. Double-click it, or pick **Expand** from the right-click menu. JSON
gets re-indented and syntax-coloured. Anything else gets a plain textarea.
`Ctrl+Enter` saves. Read-only cells, meaning primary keys and tables without one,
open in the same dialog so you can still read them.

A foreign-key value gets a **↗** next to it. Click it to jump to the table it
points at, filtered to that row. Composite foreign keys are skipped, since one
equality filter cannot express them.

Filters stack with AND and go past equality. The `contains`, `starts with` and
`ends with` operators build the LIKE pattern for you and escape any `%` or `_`
you typed literally, while raw `LIKE` and `ILIKE` leave your wildcards alone.
`IN` takes a comma-separated list. `ILIKE` is PostgreSQL-only. Everywhere else
it falls back to `LIKE`, which is already case-insensitive anyway: for ASCII on
SQLite, by collation on MySQL and SQL Server.

### Structure tab

The column list, and adding a column. The type field is a searchable dropdown of
that engine's real types, so SQLite gets its five storage classes rather than
Postgres's fifty. Anything you type that is not in the list is offered verbatim,
so `numeric(12,4)` and `text[]` both work.

### Query tab

A CodeMirror editor with autocomplete that reads your actual tables and columns.
`Ctrl+Enter`, or `⌘↵` on a Mac, runs the statement. Save a query and it lands in
the sidebar.

Everything you run is logged to History in the sidebar. The last 50 statements
per connection, newest first. Click one to load it back into the editor.
Re-running the same statement does not stack up duplicates, and **clear** empties
the list.

**Format**, or `Shift+Alt+F` and `⇧⌥F` on a Mac, pretty-prints the buffer using
the right dialect for the connection. SQL it cannot parse is left alone rather
than mangled.

While a query runs, **Run** becomes **Cancel**. On PostgreSQL and MySQL that
cancels the statement on the server itself, not only in the app. SQLite has no
server to ask, and the SQL Server driver has no cancel token, so on those it
frees the tab and leaves the statement running.

### Right-click menus

Most of the app has a context menu, because the useful actions are the ones you
would otherwise type out by hand.

- A table in the sidebar drops `SELECT * FROM …` or `SELECT count(*) FROM …` into
  the Query tab ready to run, jumps to its structure, or copies the name.
- A cell in the grid copies the value, the whole row as JSON, or the column name,
  and follows a foreign key. On editable tables it also edits the cell, sets it
  to `NULL`, or deletes the row.
- The SQL editor runs, formats, selects all, copies, saves, and clears.

### Export

CSV or JSON, from the Data tab or from a query result. Export re-runs the query
rather than dumping what is on screen, so you get the whole result instead of the
current page. Data-tab export applies the active filters and sort, so what you
filtered to is what you get, minus the page limit.

### Editing needs a primary key

To update or delete a row, the app has to name that row unambiguously. It uses
the primary key, composite keys included, matching every column at once. A table
with no primary key is read-only. The UI hides the controls and the backend
refuses anyway. Partial keys are refused too, since half a composite key matches
many rows. This is a deliberate limit rather than a missing feature. Guessing at
row identity is how a data manager corrupts data.

## Current limitations

Deliberate cuts, roughly in order of how likely you are to hit them.

- Filter values bind by declared column type, so exotic types like arrays, enums
  and ranges fall back to text comparison.
- dbelte ignores the `sslmode` and `channel_binding` URL params. sqlx defaults to
  TLS-preferred, which Neon accepts.
- Connecting gives up after 10 seconds and you cannot change that.
- Row counts are exact, so `count(*)` on a huge unfiltered table costs what it
  costs.
- `add_column` allows only alphanumerics plus `(),_[] ` in a type string, so
  anything more exotic than an array type has to go through the Query tab.
- Tables with no primary key are read-only.
- Query history is per connection, capped at 50 statements, and not searchable.

## License

MIT. See [LICENSE](LICENSE).

## Support

dbelte is free and always will be. If it saved you from another Electron
install, you can [buy me a coffee on Ko-fi](https://ko-fi.com/arthurwieb).

---

# For developers

Everything below is about building and changing dbelte rather than using it.

## Why it is built this way

Most database GUIs are a browser tab pretending to be an app, or a 400 MB
Electron install that takes ten seconds to show you a table. dbelte is a native
binary that opens a connection and shows you rows.

Three rules shape the design.

**The webview never touches your database.** Every query runs in Rust. The
frontend calls `invoke()` and gets JSON back. Connection strings, passwords and
driver handles never exist in JavaScript.

**Your passwords are not ours to keep.** Credentials go to the OS keyring, which
is Secret Service on Linux, Keychain on macOS, and Credential Manager on Windows.
The app's own metadata store holds everything except the password.

**Dynamic SQL is a boundary, not a convenience.** Filters, sorts and pagination
build real SQL, so the builder is the one part of the codebase treated as hostile
territory. It validates every identifier against the live table schema before
quoting it, and every value is a bound parameter. It has unit tests, and they are
the tests that matter most.

## Stack

| Layer | Tech |
|-------|------|
| Desktop shell | Tauri 2, custom title bar, no native decorations |
| Frontend | SvelteKit (Svelte 5 runes, `adapter-static`, SSR off), TypeScript |
| Styling | Tailwind CSS v4 and shadcn-svelte, dark-only, square corners, Svelte orange `#ff3e00` |
| Editors | CodeMirror 6 with `@codemirror/lang-sql` for schema-aware autocomplete and `@codemirror/lang-json` for expanded cells |
| DB access | Rust, `sqlx` (postgres + mysql + sqlite) and `tiberius` (SQL Server), both rustls |
| Secrets | OS keyring, `keyring` crate |
| App metadata | SQLite file in Tauri's app-data dir, `meta.db` |
| Package manager | bun, never npm |

## Development

You need [bun](https://bun.sh), [Rust](https://rustup.rs) stable, and the
platform webview toolchain.

Linux (Fedora):

```sh
sudo dnf install webkit2gtk4.1-devel gtk3-devel dbus-devel librsvg2-devel openssl-devel libappindicator-gtk3-devel patchelf
```

Linux (Debian/Ubuntu):

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libssl-dev build-essential curl wget file
```

macOS needs Xcode Command Line Tools. Windows needs the MSVC build tools and
WebView2, which ships with Windows 11.

Then:

```sh
bun install
bun run tauri dev      # launches the desktop app with HMR
```

The first run compiles the whole Rust dependency tree, so expect a few minutes.
Later runs are incremental.

> `bun run dev` alone only serves the frontend in a browser. Every database call
> goes through Tauri `invoke()`, so nothing works outside the desktop window. Use
> it for pure UI and CSS work.

Other useful commands:

```sh
bun run check                      # svelte-check (TS errors)
bun run build                      # frontend production build only (outputs to build/)
cd src-tauri && cargo test --lib   # query-builder unit tests
cd src-tauri && cargo check        # also validates capability permissions
```

A sample database, `sample.db`, with tables `users` and `orders`, sits in the
repo root for testing.

To replace the logo, drop a square transparent PNG at `static/logo.png`, then run
`bunx tauri icon static/logo.png` to regenerate every platform icon.

The screenshots in this README live in `docs/screenshots/` as `connections.png`,
`data.png` and `query.png`. Retake them against `sample.db` at a 1280x800 window
so they stay consistent.

## Building an executable

```sh
bun run tauri build
```

That runs `bun run build`, compiles Rust in release mode, then packages
installers. The first release build takes 5 to 15 minutes.

| What | Where |
|------|-------|
| Raw binary | `src-tauri/target/release/dbelte`, `.exe` on Windows |
| Installers and bundles | `src-tauri/target/release/bundle/` |

`tauri.conf.json` sets `"targets": "all"`, so each platform builds everything it
can.

- Linux gets `bundle/deb/`, `bundle/rpm/` and `bundle/appimage/`. The AppImage is
  the portable one. `chmod +x` and run.
- macOS gets `bundle/macos/dbelte.app` and `bundle/dmg/`. Unsigned builds hit
  Gatekeeper on other machines, so shipping needs an Apple Developer ID to sign
  and notarize.
- Windows gets `bundle/nsis/` and `bundle/msi/`.

For a single target instead of all:

```sh
bun run tauri build --bundles appimage    # or deb, rpm, dmg, nsis, msi
bun run tauri build --no-bundle           # binary only, skip packaging
```

### Building the AppImage on Fedora

Two things bite, and both surface as the same unhelpful message,
`failed to bundle project: failed to run linuxdeploy`, after the Rust build has
already succeeded.

1. `patchelf` must be installed, and it is in the prerequisites above.
   linuxdeploy uses it to rewrite rpaths.
2. Set `NO_STRIP=true`. linuxdeploy runs `strip` over every bundled library, and
   Fedora's binutils rejects some of what the GTK plugin pulls in, which
   linuxdeploy treats as fatal.

```sh
NO_STRIP=true bun run tauri build --bundles appimage
```

The unstripped result is around 110 MB against 8.5 MB for the `.rpm`, because the
AppImage carries its own copy of GTK and WebKit. That is the whole point of the
format. CI builds on Ubuntu, where stripping works, so the released AppImage is
smaller than a local Fedora one.

A locally built AppImage also carries the bundled `libwayland-*` that the release
workflow strips out, described below. If it aborts with
`Could not create default EGL display: EGL_BAD_PARAMETER`, that is why. Delete
those libraries from the AppDir and repack, or test with the `.rpm` instead.

Cross-compiling is not supported. Build each OS on that OS, or in CI.

### Releasing

`.github/workflows/release.yml` builds macOS universal, Linux and Windows in
parallel, then attaches the installers to a draft GitHub release. Only you can
see a draft until you publish it.

```sh
# 1. bump the version in all three manifests (they must agree)
#    package.json · src-tauri/tauri.conf.json · src-tauri/Cargo.toml
# 2. tag and push
git tag v0.2.0
git push origin v0.2.0
# 3. review the draft release on GitHub, then publish
```

`tauri-action` reads the version from `tauri.conf.json`, so a tag that disagrees
with it produces confusingly named artifacts.

Pushing an existing tag again fires nothing. To rebuild the same version, delete
the remote tag and push it back with
`git push --delete origin v0.1.3 && git push origin v0.1.3`. Running the workflow
by hand is worse than it looks, since `github.ref_name` is then `main`, which
names the release "dbelte main" and skips the tag-gated AppImage fix.

After the Linux build, the workflow unpacks the AppImage, deletes the bundled
`libwayland-*`, and repacks it. linuxdeploy bundles Ubuntu's copies, which shadow
the host's newer ones on distros like Fedora and make Mesa reject the EGL display
with `EGL_BAD_PARAMETER`. WebKit then aborts before the window appears.

macOS artifacts are unsigned, so other people hit a Gatekeeper warning until the
workflow gets an Apple Developer ID through the `APPLE_CERTIFICATE`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID` and `APPLE_PASSWORD` secrets. The `.rpm` is
unsigned too, and signing it only pays off alongside a real dnf repo.

## Architecture

All database work happens in Rust. The webview only calls `invoke()`, and
credentials never enter the frontend.

```
src/                            SvelteKit frontend
  lib/api.ts                    typed invoke() wrappers, the full command surface
  lib/dialect.ts                per-engine table: label, port, CodeMirror dialect, types
  lib/confirm.svelte.ts         promise-based confirm() backed by a themed modal
  lib/links.ts                  external URLs (Ko-fi)
  lib/cm.ts                     CodeMirror theme + highlight style, shared by both editors
  lib/components/
    Grid.svelte                 shared data grid (sort, dbl-click edit, expand dialog, row delete)
    DataTab.svelte              filters, row limit, pagination, insert/edit/delete, export
    StructureTab.svelte         column list + ALTER TABLE ADD COLUMN (searchable type picker)
    QueryTab.svelte             CodeMirror editor, run/save/export
    SqlEditor.svelte            CodeMirror 6 setup, schema autocomplete, SQL formatting
    JsonEditor.svelte           CodeMirror 6 for the expanded-cell dialog (JSON, read-only mode)
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
  db.rs                         Pool enum (Pg|Sqlite|My|Mssql), Dialect, row to JSON decoding,
                                SELECT builder (THE injection boundary, unit tested)
  mssql.rs                      the tiberius path: SQL Server shares no sqlx traits
  commands.rs                   all #[tauri::command] handlers
```

Key invariants:

- **Identifiers are never interpolated raw.** The builder validates filter and
  sort columns against the live table schema, passes identifiers through
  `quote_ident`, and binds every value as a parameter. `db.rs` unit tests cover
  this. Keep them green.
- **Editing needs exactly one PK column.** Tables without a single-column primary
  key are read-only in the UI, and the backend enforces it too.
- **An `Arc` holds pools, not the map lock.** `with_pool!` clones the `Arc` and
  releases `AppState.pools` immediately. Holding that lock for a query's duration
  would serialise every command in the app and deadlock cancellation against the
  query it is trying to cancel.
- **Query cancellation on PostgreSQL and MySQL is real.** The app opens a second
  session and calls `pg_cancel_backend` or `KILL QUERY` on the exact backend
  running your statement. SQLite
  has no server to ask, so cancelling frees the tab and discards the connection.
- **Passwords live only in the OS keyring**, under service `"dbelte"` with the
  connection UUID as the key. `meta.db` holds everything else.
- **Window controls need explicit capabilities.** `core:window:default` is
  read-only, so minimize, close, toggle-maximize and dragging are granted one by
  one in `src-tauri/capabilities/`. A missing one fails silently at runtime, and
  `cargo check` validates the identifiers.
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set on Linux in `lib.rs`. WebKitGTK's
  DMA-BUF renderer segfaults on some GPU stacks, AMD and Wayland included. Do not
  remove it.
- keyring uses the `sync-secret-service` feature on Linux. The async zbus backend
  panics inside Tauri's tokio runtime with "cannot start a runtime from within a
  runtime".

## Adding a database engine

The architecture is shaped for this. `DbPool` is an enum, so adding an arm makes
the compiler list every place that needs a decision. For SQL Server that was
exactly four.

Everything that varies by engine but not by driver lives on `Dialect` in
`db.rs`: placeholders, identifier quoting, the text cast target, ILIKE support,
pagination, the add-column keyword. Those are pure functions with unit tests, so
you can write a dialect and prove it before opening a single connection. The
frontend keeps the mirror of it in `ENGINES` in `src/lib/dialect.ts`, holding
the label, default port, CodeMirror dialect, formatter language, quoting, the
preview-query shape and the column-type catalog.

So an engine is a row in `ENGINES`, an arm on `Dialect`, an arm on `DbPool`, an
`open` branch, a value decoder, and three introspection queries. Anything sqlx
ships is a few hours of work. MySQL and MariaDB were, since the whole query,
bind and execute path generalized.

SQL Server is the interesting one, because sqlx dropped its MSSQL driver after
0.6 and never brought it back. It goes through `tiberius` instead, which shares
no traits with sqlx. No `Row`, no `query`, no pool. So `execute` and `bind_all!`
stop generalizing and `mssql.rs` is its own path end to end. That turned out to
be the smaller half of the job, and it is the nicer decoder besides: tiberius
hands back an already-decoded `ColumnData`, so `mssql.rs` matches on the value
instead of guessing from a type name the way `pg_value` has to.

Each engine has an `#[ignore]`d test in `commands.rs` that talks to a real
server in a container. The header comment on each carries its `docker run` line
and the `cargo test` invocation. Write that test first. Every engine had at
least one thing no amount of reading the docs would have caught:

- MySQL returns `information_schema` strings as `VARBINARY`, so the decoder
  hex-dumped every table name until the queries got `CAST(... AS CHAR)`.
- MySQL also processes backslashes inside string literals, which made the shared
  `ESCAPE '\'` an unterminated string. The LIKE escape character is now `!`,
  which is ordinary in every engine, so `NO_BACKSLASH_ESCAPES` cannot break it
  either.
- SQL Server rejects `OFFSET...FETCH` without an `ORDER BY`, so `Dialect` emits
  `ORDER BY (SELECT NULL)` when the grid has no sort. It has no `LIMIT` at all,
  which is why the sidebar's preview query is per-engine.

MongoDB and friends still do not fit. `build_select`, `ColumnInfo` and "exactly
one PK column" have no analogue there. That would be a separate view rather than
a `DbPool` arm.

## Gotchas learned the hard way

- `shadcn-svelte init` is interactive and fights automation, so `components.json`
  was written by hand. `shadcn-svelte add -y <components>` works fine after that.
  `add -o` overwrites existing components, so back up `src/lib/components/ui/`
  first.
- Svelte 5: do not read reactive state inside the `$effect` that creates
  CodeMirror. Every keystroke recreates the editor and drops focus. It is created
  once in `onMount` with `untrack`.
- Tauri v2 auto-converts command arg names between Rust `snake_case` and JS
  `camelCase`. `default` is a Rust keyword, so the add-column arg is
  `default_value`.
- `sqlx` with `default-features = false` needs the `macros` feature for
  `#[derive(FromRow)]`.
- An undecorated window loses the WM's resize border, hence `ResizeGrips.svelte`.
  Without it the window is stuck at its initial size on Linux.
- External links must go through `@tauri-apps/plugin-opener`. A plain `<a href>`
  navigates the webview away from the app.
- The crate is named `dbelte`, not the template's `app`. That name becomes the
  Linux binary, `Exec=`, `StartupWMClass`, and the icon filenames.
