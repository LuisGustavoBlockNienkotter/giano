"""User settings stored in the database."""

from __future__ import annotations

from pathlib import Path

from giano.config import DEFAULT_CLAUDE_PROMPT, default_export_dir
from giano.db import Database
from giano.db import settings as repo

EXPORT_DIR = "export_dir"
CLAUDE_PROMPT = "claude_prompt"

KEYS = {
    EXPORT_DIR: "Pasta de exportação",
    CLAUDE_PROMPT: "Template do pedido de análise (exportação para o Claude)",
}


class SettingsService:
    def __init__(self, db: Database):
        self.db = db

    @property
    def export_dir(self) -> Path:
        value = repo.get(self.db, EXPORT_DIR)
        return Path(value).expanduser() if value else default_export_dir()

    @export_dir.setter
    def export_dir(self, value: str | Path | None) -> None:
        with self.db.transaction():
            if value in (None, ""):
                repo.delete(self.db, EXPORT_DIR)
            else:
                repo.set(self.db, EXPORT_DIR, str(value))

    @property
    def claude_prompt(self) -> str:
        return repo.get(self.db, CLAUDE_PROMPT) or DEFAULT_CLAUDE_PROMPT

    @claude_prompt.setter
    def claude_prompt(self, value: str | None) -> None:
        with self.db.transaction():
            if not value or not value.strip() or value.strip() == DEFAULT_CLAUDE_PROMPT.strip():
                repo.delete(self.db, CLAUDE_PROMPT)
            else:
                repo.set(self.db, CLAUDE_PROMPT, value)
