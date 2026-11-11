"""Full-text search across current items and notes (SQLite FTS5)."""

from __future__ import annotations

from dataclasses import dataclass

from giano.db import Database
from giano.db import search as repo
from giano.domain import ItemStatus, NoteStatus


@dataclass(frozen=True)
class SearchHit:
    kind: str  # "item" | "note"
    id: int  # item id or note id
    project_id: int | None
    label: str  # "MERC-12" or "2026-11-10"
    title: str
    status: str  # user-facing label
    snippet: str


class SearchService:
    def __init__(self, db: Database):
        self.db = db

    def search(self, text: str, project_id: int | None = None, limit: int = 100) -> list[SearchHit]:
        hits: list[SearchHit] = []
        for r in repo.search_items(self.db, text, project_id, limit):
            hits.append(
                SearchHit("item", r["item_id"], r["project_id"], r["code"], r["title"],
                          ItemStatus(r["status"]).label, _flat(r["snippet"]))
            )
        for r in repo.search_notes(self.db, text, project_id, limit):
            label = r["note_date"] + (f" · {r['project_code']}" if r["project_code"] else "")
            hits.append(
                SearchHit("note", r["note_id"], None, label, r["name"],
                          NoteStatus(r["status"]).label, _flat(r["snippet"]))
            )
        return hits


def _flat(s: str | None) -> str:
    return " ".join((s or "").split())
