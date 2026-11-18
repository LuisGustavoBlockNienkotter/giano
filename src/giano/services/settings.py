"""User settings.

The Claude prompt is stored in the database (shared by every computer). The export folder
belongs to the computer: with a Turso database it is kept in the local ``config.json``.
"""

from __future__ import annotations

from pathlib import Path

from giano import config
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
        if self.db.remote:
            value = config.load_config().get(config.EXPORT_DIR)
        else:
            value = repo.get(self.db, EXPORT_DIR)
        return Path(value).expanduser() if value else default_export_dir()

    @export_dir.setter
    def export_dir(self, value: str | Path | None) -> None:
        if self.db.remote:
            config.save_config(**{config.EXPORT_DIR: str(value) if value else None})
            return
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
