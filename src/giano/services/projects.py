"""Project service. Projects are not part of the versioned list."""

from __future__ import annotations

import sqlite3
from collections.abc import Callable
from datetime import datetime

from giano.db import Database
from giano.db import projects as repo
from giano.domain import Project, ValidationError, normalize_body, normalize_code


class ProjectService:
    def __init__(self, db: Database, clock: Callable[[], datetime] = datetime.now):
        self.db = db
        self.clock = clock

    def list(self, include_archived: bool = True) -> list[Project]:
        return repo.list_all(self.db, include_archived)

    def get(self, project_id: int) -> Project | None:
        return repo.get(self.db, project_id)

    def get_by_code(self, code: str) -> Project | None:
        return repo.get_by_code(self.db, code.strip())

    def require(self, code: str) -> Project:
        project = self.get_by_code(code)
        if project is None:
            raise ValidationError(f"Projeto '{code}' não encontrado.")
        return project

    def create(self, code: str, name: str, description: str = "", active: bool = True) -> Project:
        code, name = normalize_code(code), name.strip()
        if not name:
            raise ValidationError("O nome do projeto é obrigatório.")
        with self.db.transaction():
            if repo.get_by_code(self.db, code):
                raise ValidationError(f"Já existe um projeto com a sigla {code}.")
            pid = repo.insert(
                self.db, code, name, normalize_body(description), active,
                self.clock().isoformat(timespec="seconds"),
            )
        return repo.get(self.db, pid)  # type: ignore[return-value]

    def update(
        self, project_id: int, code: str, name: str, description: str, active: bool
    ) -> Project:
        """Edit a project. The code may change: item identifiers follow it
        (they are always built from the current code)."""
        code, name = normalize_code(code), name.strip()
        if not name:
            raise ValidationError("O nome do projeto é obrigatório.")
        with self.db.transaction():
            other = repo.get_by_code(self.db, code)
            if other and other.id != project_id:
                raise ValidationError(f"Já existe um projeto com a sigla {code}.")
            try:
                repo.update(self.db, project_id, code, name, normalize_body(description), active)
            except sqlite3.IntegrityError as exc:  # pragma: no cover - guarded above
                raise ValidationError(str(exc)) from exc
        return repo.get(self.db, project_id)  # type: ignore[return-value]

    def set_active(self, project_id: int, active: bool) -> Project:
        p = self.get(project_id)
        if p is None:
            raise ValidationError("Projeto não encontrado.")
        return self.update(p.id, p.code, p.name, p.description, active)
