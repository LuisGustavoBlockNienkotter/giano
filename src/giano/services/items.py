"""Item service: the revision rules of a project's list.

Every change to a project's list (create, edit, delete, status/priority/tags)
produces exactly one new revision, inside a single transaction, using
append-only temporal versions:

* the current version is closed with ``rev_end = N + 1``;
* the new content is inserted with ``rev_start = N + 1``;
* deleting only closes the current version.

Archiving is a versioned change too: the new version is a copy of the current
content with ``archived = True`` (restoring does the opposite). Archived items
keep everything they had, but they are read-only until restored and stay out of
the normal list and of the exports.

Saving without a real change does not create a revision.

Addenda: an addendum (``MERC-33.1``) is an item of its own, created from an
existing item to record a follow-up (a fix or an adjustment that came back from
development) without touching the original. It keeps the original's number,
has its own content, status, revisions and archive state, and is managed
independently: the original can be finished or archived while the addendum stays
on the list. The original cannot be deleted while it has addenda.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import replace
from datetime import datetime

from giano.db import Database
from giano.db import items as repo
from giano.db import projects as projects_repo
from giano.domain import (
    ItemChange,
    ItemData,
    ItemStatus,
    ItemView,
    Priority,
    Project,
    Revision,
    RevisionDiff,
    ValidationError,
    compare_items,
)


class ItemService:
    def __init__(self, db: Database, clock: Callable[[], datetime] = datetime.now):
        self.db = db
        self.clock = clock

    def _now(self) -> str:
        return self.clock().isoformat(timespec="seconds")

    def _project(self, project_id: int) -> Project:
        project = projects_repo.get(self.db, project_id)
        if project is None:
            raise ValidationError("Projeto não encontrado.")
        return project

    # -- revisions --------------------------------------------------------

    def latest_revision(self, project_id: int) -> int:
        return repo.latest_revision_number(self.db, project_id)

    def revisions(self, project_id: int) -> list[Revision]:
        """All revisions of a project, newest first."""
        return repo.list_revisions(self.db, project_id)

    def revision(self, project_id: int, number: int) -> Revision | None:
        return repo.get_revision(self.db, project_id, number)

    def _new_revision(self, project_id: int, summary: str, item_id: int | None) -> int:
        number = repo.latest_revision_number(self.db, project_id) + 1
        repo.insert_revision(self.db, project_id, number, self._now(), summary, item_id)
        return number

    # -- queries ----------------------------------------------------------

    def list_items(
        self, project_id: int, rev: int | None = None, archived: bool | None = None
    ) -> list[ItemView]:
        """The list at revision ``rev`` (default: latest).

        ``archived``: ``None`` = every item, ``False`` = only the normal list,
        ``True`` = only the archived items.
        """
        if rev is None:
            rev = self.latest_revision(project_id)
        items = repo.list_at_revision(self.db, project_id, rev)
        if archived is not None:
            items = [v for v in items if v.data.archived == archived]
        return items

    def get(self, item_id: int, rev: int | None = None) -> ItemView | None:
        if rev is None:
            return repo.get_current(self.db, item_id)
        return repo.get_at_revision(self.db, item_id, rev)

    def get_by_code(self, code: str) -> ItemView | None:
        """Find a current item by its identifier, e.g. ``MERC-12`` or ``MERC-12.1``."""
        prefix, _, ref = code.strip().rpartition("-")
        number, _, addendum = ref.partition(".")
        if not prefix or not number.isdigit() or (addendum and not addendum.isdigit()):
            return None
        project = projects_repo.get_by_code(self.db, prefix)
        if project is None:
            return None
        return repo.get_current_by_number(self.db, project.id, int(number), int(addendum or 0))

    def parent(self, view: ItemView, rev: int | None = None) -> ItemView | None:
        """The original item of an addendum, at revision ``rev`` (default: latest)."""
        if not view.is_addendum:
            return None
        if rev is None:
            return repo.get_current_by_number(self.db, view.project_id, view.number)
        return repo.get_by_number_at_revision(self.db, view.project_id, view.number, 0, rev)

    def addenda(self, view: ItemView, rev: int | None = None) -> list[ItemView]:
        """The addenda of an item (archived ones included), at revision ``rev``."""
        if rev is None:
            rev = self.latest_revision(view.project_id)
        return repo.addenda_at_revision(self.db, view.project_id, view.number, rev)

    def history(self, item_id: int) -> list[ItemView]:
        return repo.item_history(self.db, item_id)

    def all_tags(self) -> list[str]:
        return repo.all_tags(self.db)

    # -- changes ----------------------------------------------------------

    def create(
        self, project_id: int, data: ItemData, source_note_id: int | None = None
    ) -> ItemView:
        data = data.normalized()
        with self.db.transaction():
            project = self._project(project_id)
            if not project.active:
                raise ValidationError("Projeto arquivado: não é possível criar itens.")
            number = repo.next_item_number(self.db, project_id)
            item_id = repo.insert_item(self.db, project_id, number, source_note_id, self._now())
            code = f"{project.code}-{number}"
            rev = self._new_revision(project_id, f"{code} criado: {data.title}", item_id)
            repo.insert_version(self.db, item_id, rev, data)
            return repo.get_current(self.db, item_id)  # type: ignore[return-value]

    def create_addendum(
        self, item_id: int, data: ItemData, source_note_id: int | None = None
    ) -> ItemView:
        """Create an addendum of an item (``MERC-33`` → ``MERC-33.1``), one new revision.

        Asking for an addendum of an addendum attaches it to the same original item
        (``MERC-33.1`` → ``MERC-33.2``).
        """
        data = data.normalized()
        with self.db.transaction():
            base = repo.get_current(self.db, item_id)
            if base is None:
                raise ValidationError("Item não encontrado ou excluído.")
            project = self._project(base.project_id)
            if not project.active:
                raise ValidationError("Projeto arquivado: não é possível criar adendos.")
            original = (
                repo.get_current_by_number(self.db, base.project_id, base.number)
                if base.is_addendum else base
            )
            if original is None:
                raise ValidationError(f"O item original {base.parent_code} foi excluído.")
            addendum = repo.next_addendum_number(self.db, base.project_id, base.number)
            new_id = repo.insert_item(
                self.db, base.project_id, base.number, source_note_id, self._now(), addendum
            )
            code = f"{original.code}.{addendum}"
            rev = self._new_revision(
                base.project_id, f"{code} criado (adendo de {original.code}): {data.title}", new_id
            )
            repo.insert_version(self.db, new_id, rev, data)
            return repo.get_current(self.db, new_id)  # type: ignore[return-value]

    def update(self, item_id: int, data: ItemData) -> ItemView | None:
        """Save new content. Returns the new view, or ``None`` if nothing changed."""
        data = data.normalized()
        with self.db.transaction():
            current = repo.get_current(self.db, item_id)
            if current is None:
                raise ValidationError("Item não encontrado ou excluído.")
            if current.data.archived:
                raise ValidationError(
                    f"{current.code} está arquivado: restaure-o para a lista antes de editar."
                )
            # Archiving has its own operation; an edit never archives or restores.
            return self._save(current, replace(data, archived=False))

    def _save(self, current: ItemView, data: ItemData) -> ItemView | None:
        changes = compare_items(current.data, data)
        if not changes:
            return None
        summary = f"{current.code}: " + "; ".join(c.describe() for c in changes)
        rev = self._new_revision(current.project_id, summary, current.item_id)
        repo.close_version(self.db, current.version_id, rev)
        repo.insert_version(self.db, current.item_id, rev, data)
        return repo.get_current(self.db, current.item_id)

    def archive(self, item_id: int) -> ItemView:
        """Move an item to the archive, keeping all its content (new revision)."""
        return self._set_archived(item_id, True)

    def unarchive(self, item_id: int) -> ItemView:
        """Bring an archived item back to the normal list (new revision)."""
        return self._set_archived(item_id, False)

    def _set_archived(self, item_id: int, archived: bool) -> ItemView:
        with self.db.transaction():
            current = repo.get_current(self.db, item_id)
            if current is None:
                raise ValidationError("Item não encontrado ou excluído.")
            if current.data.archived == archived:
                state = "já está arquivado" if archived else "não está arquivado"
                raise ValidationError(f"{current.code} {state}.")
            if not self._project(current.project_id).active:
                raise ValidationError("Projeto arquivado: somente leitura.")
            return self._save(current, replace(current.data, archived=archived))  # type: ignore[return-value]

    def set_status(self, item_id: int, status: ItemStatus) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, status=status))

    def set_priority(self, item_id: int, priority: Priority) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, priority=priority))

    def set_tags(self, item_id: int, tags) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, tags=tags))

    def delete(self, item_id: int) -> int:
        """Delete an item (closes its current version). Returns the new revision number."""
        with self.db.transaction():
            current = repo.get_current(self.db, item_id)
            if current is None:
                raise ValidationError("Item não encontrado ou já excluído.")
            if not current.is_addendum:
                rev_now = repo.latest_revision_number(self.db, current.project_id)
                addenda = repo.addenda_at_revision(
                    self.db, current.project_id, current.number, rev_now
                )
                if addenda:
                    codes = ", ".join(a.code for a in addenda)
                    raise ValidationError(
                        f"{current.code} tem adendos ({codes}): exclua-os antes, "
                        "ou arquive o item em vez de excluir."
                    )
            rev = self._new_revision(
                current.project_id, f"{current.code} excluído: {current.data.title}", item_id
            )
            repo.close_version(self.db, current.version_id, rev)
            return rev

    def _require(self, item_id: int) -> ItemView:
        current = repo.get_current(self.db, item_id)
        if current is None:
            raise ValidationError("Item não encontrado ou excluído.")
        return current

    # -- diff -------------------------------------------------------------

    def diff(
        self, project_id: int, rev_from: int, rev_to: int, include_archived: bool = True
    ) -> RevisionDiff:
        """Differences going from revision ``rev_from`` to ``rev_to``.

        With ``include_archived=False`` archived items count as absent: archiving
        shows up as a removal and restoring as an addition (used by the HTML export).
        """
        project = self._project(project_id)
        latest = self.latest_revision(project_id)
        for r in (rev_from, rev_to):
            if not 0 <= r <= latest:
                raise ValidationError(f"Revisão {r} não existe (última: {latest}).")
        only = None if include_archived else False
        before = {v.item_id: v for v in self.list_items(project_id, rev_from, only)}
        after = {v.item_id: v for v in self.list_items(project_id, rev_to, only)}
        result = RevisionDiff(project=project, rev_from=rev_from, rev_to=rev_to)
        for item_id, view in after.items():
            old = before.get(item_id)
            if old is None:
                result.added.append(view)
            elif old.version_id != view.version_id:
                changes = compare_items(old.data, view.data)
                if changes:  # changed and changed back = no difference
                    result.changed.append(ItemChange(old, view, changes))
        result.removed = [v for item_id, v in before.items() if item_id not in after]
        for lst in (result.added, result.removed):
            lst.sort(key=lambda v: v.key)
        result.changed.sort(key=lambda c: c.new.key)
        return result
