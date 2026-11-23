# CLAUDE.md

Giano: TUI (Textual) to manage improvement lists per project, versioned by revisions.
The database is a local SQLite file or a Turso (libSQL) database shared by several computers.
The full product spec is in `SPEC.md`.

## Product direction (READ FIRST)

- **The Textual TUI is FROZEN** (`src/giano/tui/`, `giano.tcss`): no new features, no UI
  changes, no refactors. Only touch it if a change elsewhere breaks it (keep it running).
- **New features and UI work go to the desktop app** (`desktop/`, Tauri + Rust + Svelte).
- **The CLI stays maintained** (`src/giano/cli.py`, `giano item ...` / `giano itens`, and the
  `services/` + `db/` it uses): Claude's user-level `giano` skill drives it to file items
  automatically. Keep its commands and output stable; extend it when a desktop feature
  needs to be reachable from automation.
- Both stacks share the database: a schema change still goes to BOTH migration files with
  the same version number (see the Desktop section), or the CLI breaks on the shared DB.

## Commands

```bash
uv sync                          # install deps (incl. dev group: pytest)
uv run giano                     # run the TUI (uses GIANO_DB or the user data dir)
GIANO_DB=/tmp/x.db uv run giano demo   # sample data in a throwaway DB
uv run python -m pytest          # all tests (use `python -m pytest`: some Windows
                                 # machines block the pytest.exe shim)
uv run python -m pytest tests/test_revisions.py -k delete
```

Python is pinned to 3.12 by `.python-version`: `textual[syntax]` needs `tree-sitter`
wheels, which don't exist yet for the newest Python on Windows (uv would try to compile
them and fail without MSVC).

## Layout

```
src/giano/
  domain.py          enums (pt-BR labels), dataclasses, ItemData normalization, compare_items
  config.py          DB target resolution (--db/GIANO_DB > GIANO_TURSO_* env > config.json >
                     local file), per-computer config.json (Turso URL/token, export_dir)
  db/                ONLY place with SQL
    connection.py    Database: autocommit conn + transaction() (BEGIN IMMEDIATE, nestable);
                     a libsql:// URL uses remote.py instead of sqlite3
    remote.py        RemoteConnection: libsql driver behind the sqlite3 surface (Row with
                     row["col"], :name -> ?N params, errors -> sqlite3.IntegrityError/OperationalError)
    transfer.py      copy all rows into an empty DB (`giano turso enviar`)
    migrations.py    MIGRATIONS list + schema_version table (append new versions, never edit)
    projects.py items.py notes.py settings.py search.py   repositories (plain functions)
  services/          business rules; the UI and CLI only call these
    __init__.py      Giano facade (projects, items, notes, settings, search, exports)
    items.py         revision rules: create/update/delete, list at revision, diff
    notes.py projects.py settings.py search.py
    exports.py       Claude Markdown export + HTML export (Jinja2 template)
    markdown.py      markdown-it-py + Pygments rendering, heading demotion
    formatting.py    Markdown previews/diffs for TUI and CLI
    demo.py          `giano demo` data
    clipboard.py     CLI clipboard (TUI uses App.copy_to_clipboard)
  templates/export.html.j2   self-contained HTML (inline CSS + Pygments CSS)
  tui/               Textual screens; app.py (GianoApp), main_screen.py, item_editor.py,
                     notes.py (NoteEditor autosave, QuickNoteScreen), revisions.py,
                     search.py, projects.py, dialogs.py, widgets.py, giano.tcss
  cli.py             Typer app (entry point `giano`); no subcommand = TUI; `giano item ...`
                     / `giano itens` = non-interactive item commands (incl. `item adendo`) (used by a user-level
                     `giano` skill in ~/.claude/skills so Claude can file items from any folder)
tests/               pytest; test_tui.py drives the app with Textual's pilot. The `g` fixture
                     runs every test twice: sqlite3 and the libsql driver on a local file
                     (same adapter as Turso, no network). Config is isolated per test.
```

## Desktop app (`desktop/`, Tauri 2 + Rust + Svelte 5)

A separate Windows app; the Python TUI/CLI above keeps working unchanged. It shares the SAME
database and `config.json` (same paths as `platformdirs`), so both can run side by side.

```
desktop/
  crates/giano-core/   Rust port of domain.py + db/ + services/ (rusqlite bundled for local
                       files, a small Hrana-over-HTTP client in db/remote.rs for Turso,
                       pulldown-cmark + syntect for Markdown, minijinja for export.html.j2)
    tests/             port of the pytest service tests (`cargo test -p giano-core`)
  src-tauri/           Tauri app: commands.rs = thin async commands over giano-core
                       (spawn_blocking, errors as {kind, message}, `progress` events)
  src/                 Svelte 5 (runes) + Tailwind 4: lib/state.svelte.ts (global store),
                       lib/api.ts (typed invoke), views/ (one file per screen/dialog)
```

- Commands (in `desktop/`): `npm run tauri dev`, `npm run tauri build`,
  `cargo test -p giano-core`, `npm run check`.
- Same rules as the Python core. Schema changes go to BOTH `db/migrations.py` and
  `desktop/crates/giano-core/src/db/migrations.rs` with the same version number.
- Rust SQL uses positional `?` / `?N` params (`params![...]`); `Database::transaction`
  takes a closure (nestable); `Database::session` keeps one Turso stream.
- The UI never formats domain labels itself: `meta` gives the enum labels; Markdown is
  rendered by the `render_markdown` command (same renderer as the HTML export).

## Revision model (core invariant)

- `items` = stable identity (project_id, number, source_note_id, created_at). Never deleted;
  the only UPDATE is `items.control_date` (the user's control date, NOT versioned: no
  revision, same at every revision; set to today by the HTML export with `control_date`,
  or by `ItemService.set_control_date` / `giano item controle`).
- `item_versions` = content with `rev_start`/`rev_end` (NULL = current). Append-only:
  the only UPDATE allowed is closing a version (`rev_end = N+1`).
- List at revision N: `rev_start <= N AND (rev_end IS NULL OR rev_end > N)`.
- Every change goes into a row of `revisions` (number sequential per project, summary text),
  inside one `db.transaction()`. Delete only closes the current version.
- A revision may hold N changes (`revision_changes`: one line per item, its net change;
  `revisions.item_id` is legacy, no longer written). Changes go to `ItemService._target`:
  the project's OPEN revision (`is_open = 1`, always the latest; `open_revision` /
  `close_revision`, CLI `giano revisao abrir|fechar|ver`), else the current
  `ItemService.batch()` (bulk ops: `set_status_many`, export marking), else a new revision
  of its own. In a collecting revision an item changed again has its version from THAT
  revision deleted and rewritten (the only DELETE on `item_versions`, since that revision is
  not finished); going back to the previous content reopens the old version and drops the
  change line. Closing an empty revision deletes it.
- Saving with no real change (after `ItemData.normalized()`) must NOT create a revision.
- Tags link to versions (`item_version_tags`), not to items.
- Archiving = a new version with `archived = 1` (`ItemData.archived`), via
  `ItemService.archive/unarchive`; `update()` refuses archived items and never changes the
  flag. `list_items(archived=False)` is the normal list; exports (HTML, Claude) never
  include archived items (HTML diff uses `include_archived=False`).
- Addenda: an addendum is an `items` row with the original's `number` and `addendum > 0`
  (code `MERC-33.1`; `addendum = 0` is a normal item; `ItemView.key = (number, addendum)`).
  It has its own versions/status/archive and is created by `ItemService.create_addendum`
  (an addendum of an addendum attaches to the original). Deleting an item that has addenda
  is refused. Item selections (HTML export, `--itens 1,3-5,33.1`) use keys, see
  `domain.parse_item_keys` and `exports.selection_keys`.
- Notes are not versioned. `item_fts` (rowid = version id) is written by the repository;
  `note_fts` is maintained by triggers. Search joins `rev_end IS NULL`.

## Conventions

- Code/identifiers in English; all user-facing text in Portuguese (pt-BR).
- Enum values stored in English (`in_progress`); `.label` gives the Portuguese text.
- User-facing errors: raise `domain.ValidationError` with a pt-BR message.
- UI never imports `giano.db`; go through `app.giano` (the `Giano` facade).
- DataTable cells with user text must use `widgets.plain()` (strings are parsed as markup).
- F5 (`GianoApp.action_refresh_data`) calls `refresh_data() -> str | None` on every screen
  in the stack that defines it; new screens that show DB data should implement it. Editors
  never drop unsaved input (item editor confirms before overwriting an external change).
- Services take a `clock` callable so tests are deterministic (`tests/conftest.py::FakeClock`).
- SQL must work on both drivers: stick to the `Database` methods (execute/query/query_one/
  transaction); don't use sqlite3-only APIs (row_factory, create_function, backup...).
  On Turso every statement is a network round trip (~70-200 ms): prefer one query to N+1,
  and reuse what a screen already loaded (MainScreen previews from `all_items`, so moving
  the cursor does no query). Network/auth failures raise `DatabaseUnavailable`, a
  `ValidationError`: the UI shows it like any refused operation.
- `settings` in the DB are shared by every computer; per-computer values (export_dir with
  Turso) go to `config.json` (`config.load_config/save_config`).
- Schema changes: add `(N, sql)` to `MIGRATIONS`; add a test. Migrations run with foreign
  keys off and are checked with `PRAGMA foreign_key_check` before commit, so a script may
  rebuild a table (V3 rebuilt `items`).
