# CLAUDE.md

Giano: TUI (Textual) to manage improvement lists per project, versioned by revisions.
The full product spec is in `SPEC.md`.

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
  config.py          DB/export locations, default Claude prompt
  db/                ONLY place with SQL
    connection.py    Database: autocommit conn + transaction() (BEGIN IMMEDIATE, nestable)
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
  cli.py             Typer app (entry point `giano`); no subcommand = TUI
tests/               pytest; test_tui.py drives the app with Textual's pilot
```

## Revision model (core invariant)

- `items` = stable identity (project_id, number, source_note_id). Never updated/deleted.
- `item_versions` = content with `rev_start`/`rev_end` (NULL = current). Append-only:
  the only UPDATE allowed is closing a version (`rev_end = N+1`).
- List at revision N: `rev_start <= N AND (rev_end IS NULL OR rev_end > N)`.
- Every change = one row in `revisions` (number sequential per project, summary text),
  inside one `db.transaction()`. Delete only closes the current version.
- Saving with no real change (after `ItemData.normalized()`) must NOT create a revision.
- Tags link to versions (`item_version_tags`), not to items.
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
- Schema changes: add `(N, sql)` to `MIGRATIONS`; add a test.
