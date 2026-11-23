"""Item service: the revision rules of a project's list.

Every change to a project's list (create, edit, delete, status/priority/tags) goes
into a revision, inside a single transaction, using append-only temporal versions:

* the current version is closed with ``rev_end = N``;
* the new content is inserted with ``rev_start = N``;
* deleting only closes the current version.

A revision may hold several changes (a block of changes = one revision):

* an OPEN revision (``open_revision`` … ``close_revision``) collects every change made
  to the project, from any screen or computer, until it is closed;
* a batch (``with items.batch():``) puts the changes of one operation that touches
  several items (status of several items, export marking items) into one revision;
* any other change creates a revision of its own.

A revision that collects changes keeps only the NET change of each item: changing an
item again rewrites the version created by that revision (the only case where a version
is deleted: it was never part of a finished revision), and an item that goes back to
what it was leaves the revision. ``revision_changes`` has one line per item changed.

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

from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass, field, replace
from datetime import date, datetime

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


@dataclass
class _Target:
    """The revision that receives a change."""

    project_id: int
    number: int
    id: int | None  # None: the row is written together with the first change
    collecting: bool  # an open revision or a batch: several changes, net per item
    is_open: bool = False


@dataclass
class Batch:
    """Handle of ``ItemService.batch``: ``summary`` may be set inside the block."""

    summary: str = ""
    targets: dict[int, _Target] = field(default_factory=dict)


def _summary(text: str, lines: list[str]) -> str:
    """The summary of a revision: the given text, or its only change, or a count."""
    if text:
        return text
    return lines[0] if len(lines) == 1 else f"{len(lines)} itens alterados"


class ItemService:
    def __init__(self, db: Database, clock: Callable[[], datetime] = datetime.now):
        self.db = db
        self.clock = clock
        self._batch: Batch | None = None

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
        """All revisions of a project, newest first, with their changes."""
        return repo.list_revisions(self.db, project_id)

    def revision(self, project_id: int, number: int) -> Revision | None:
        return repo.get_revision(self.db, project_id, number)

    def get_open_revision(self, project_id: int) -> Revision | None:
        """The open revision of a project (always its latest), if any."""
        return repo.get_open_revision(self.db, project_id)

    def open_revision(self, project_id: int, summary: str = "") -> Revision:
        """Open a revision: every change to the project goes into it (one revision for a
        whole block of changes) until ``close_revision``."""
        with self.db.transaction():
            if not self._project(project_id).active:
                raise ValidationError("Projeto arquivado: somente leitura.")
            last = repo.last_revision(self.db, project_id)
            if last and last[2]:
                raise ValidationError(
                    f"A Rev. {last[1]} já está aberta: feche-a antes de abrir outra."
                )
            number = (last[1] if last else 0) + 1
            repo.insert_revision(
                self.db, project_id, number, self._now(), summary.strip(), is_open=True
            )
            return repo.get_revision(self.db, project_id, number)  # type: ignore[return-value]

    def close_revision(self, project_id: int, summary: str | None = None) -> Revision | None:
        """Close the open revision (its date becomes the closing time). ``summary``
        replaces the one given when opening; without any, the summary is the only change
        or a count. A revision that ended up with no change is removed (returns ``None``)."""
        with self.db.transaction():
            rev = repo.get_open_revision(self.db, project_id)
            if rev is None:
                raise ValidationError("Não há revisão aberta neste projeto.")
            last = repo.last_revision(self.db, project_id)
            assert last is not None
            lines = [c.summary for c in rev.changes]
            if not lines:
                repo.delete_revision(self.db, last[0])
                return None
            text = rev.summary if summary is None else summary.strip()
            repo.update_revision(self.db, last[0], _summary(text, lines), False, self._now())
            return repo.get_revision(self.db, project_id, rev.number)

    @contextmanager
    def batch(self, summary: str = "") -> Iterator[Batch]:
        """Put every change made inside the block into ONE revision per project, in one
        transaction (all or nothing). With an open revision the changes go into it.
        ``summary`` (also settable on the yielded handle) is used when more than one
        item changes; a single change keeps its own description. Nested calls join the
        outer batch."""
        if self._batch is not None:
            yield self._batch
            return
        with self.db.transaction():
            batch = self._batch = Batch(summary)
            try:
                yield batch
            finally:
                self._batch = None
            for t in batch.targets.values():
                if t.id is not None and not t.is_open:
                    lines = repo.change_lines(self.db, t.id)
                    if not lines:
                        repo.delete_revision(self.db, t.id)
                    else:
                        text = batch.summary if len(lines) > 1 else ""
                        repo.update_revision(self.db, t.id, _summary(text, lines), False)

    def _target(self, project_id: int) -> _Target:
        """The revision a change of ``project_id`` goes into (one query)."""
        if self._batch is not None and project_id in self._batch.targets:
            return self._batch.targets[project_id]
        last = repo.last_revision(self.db, project_id)
        if last and last[2]:
            target = _Target(project_id, last[1], last[0], collecting=True, is_open=True)
        else:
            number = (last[1] if last else 0) + 1
            target = _Target(project_id, number, None, collecting=self._batch is not None)
        if self._batch is not None:
            self._batch.targets[project_id] = target
        return target

    def _record(self, target: _Target, item_id: int, line: str | None) -> None:
        """Record the net change of an item in ``target`` (``None``: no net change)."""
        if target.id is None:
            if line is None:
                return
            summary = "" if target.collecting else line
            target.id = repo.insert_revision(
                self.db, target.project_id, target.number, self._now(), summary
            )
        if line is None:
            repo.delete_change(self.db, target.id, item_id)
        else:
            repo.set_change(self.db, target.id, item_id, line)

    @staticmethod
    def _created_line(view: ItemView, title: str) -> str:
        if view.is_addendum:
            return f"{view.code} criado (adendo de {view.parent_code}): {title}"
        return f"{view.code} criado: {title}"

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
            target = self._target(project_id)
            number = repo.next_item_number(self.db, project_id)
            item_id = repo.insert_item(self.db, project_id, number, source_note_id, self._now())
            repo.insert_version(self.db, item_id, target.number, data)
            self._record(target, item_id, f"{project.code}-{number} criado: {data.title}")
            return repo.get_current(self.db, item_id)  # type: ignore[return-value]

    def create_addendum(
        self, item_id: int, data: ItemData, source_note_id: int | None = None
    ) -> ItemView:
        """Create an addendum of an item (``MERC-33`` → ``MERC-33.1``).

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
            target = self._target(base.project_id)
            addendum = repo.next_addendum_number(self.db, base.project_id, base.number)
            new_id = repo.insert_item(
                self.db, base.project_id, base.number, source_note_id, self._now(), addendum
            )
            repo.insert_version(self.db, new_id, target.number, data)
            new = repo.get_current(self.db, new_id)
            assert new is not None
            self._record(target, new_id, self._created_line(new, data.title))
            return new

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
        target = self._target(current.project_id)
        if current.rev_start == target.number:
            # Already changed in this revision (open revision or batch): keep only the
            # net change, rewriting the version this revision created.
            before = repo.get_at_revision(self.db, current.item_id, target.number - 1)
            repo.delete_version(self.db, current.version_id)
            if before is not None and not compare_items(before.data, data):
                # Back to what it was: the item leaves the revision.
                repo.reopen_version(self.db, before.version_id)
                self._record(target, current.item_id, None)
                return repo.get_current(self.db, current.item_id)
            repo.insert_version(self.db, current.item_id, target.number, data)
            if before is None:  # created in this revision
                line = self._created_line(current, data.title)
            else:
                changes = compare_items(before.data, data)
                line = f"{current.code}: " + "; ".join(c.describe() for c in changes)
        else:
            repo.close_version(self.db, current.version_id, target.number)
            repo.insert_version(self.db, current.item_id, target.number, data)
            line = f"{current.code}: " + "; ".join(c.describe() for c in changes)
        self._record(target, current.item_id, line)
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

    # -- control date (not versioned) ---------------------------------------

    def set_control_date(self, item_ids: list[int], day: str | None) -> None:
        """Set (``YYYY-MM-DD``) or clear (``None``) the control date of these items. It is
        not part of the item's content: no revision, the same at every revision."""
        if day is not None:
            try:
                day = date.fromisoformat(day.strip()).isoformat()
            except ValueError:
                raise ValidationError(f"Data de controle inválida: '{day}'.") from None
        if not item_ids:
            return
        with self.db.transaction():
            # The export sets it with the repository directly (no query per item).
            for project_id in {self._require(i).project_id for i in item_ids}:
                if not self._project(project_id).active:
                    raise ValidationError("Projeto arquivado: somente leitura.")
            repo.set_control_date(self.db, list(item_ids), day)

    def set_status(self, item_id: int, status: ItemStatus) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, status=status))

    def set_status_many(
        self,
        item_ids: list[int],
        status: ItemStatus,
        progress: Callable[[int, int], None] | None = None,
    ) -> list[ItemView]:
        """Change the status of several items in ONE revision (per project; with an open
        revision, into it), skipping items already in ``status``, all in one transaction:
        either every item changes or none does. Returns the new views, in the order of
        ``item_ids``.

        ``progress(done, total)`` is called before the first change and after each one
        (``total`` = items that change): on Turso every change is several round trips."""
        changed: list[ItemView] = []
        with self.batch() as batch:
            todo = []
            for item_id in dict.fromkeys(item_ids):
                current = self._require(item_id)
                if current.data.archived:
                    raise ValidationError(
                        f"{current.code} está arquivado: restaure-o para a lista antes de editar."
                    )
                if current.data.status != status:
                    todo.append(current)
            batch.summary = f"{len(todo)} itens → {status.label}"
            if progress:
                progress(0, len(todo))
            for current in todo:
                # _save (not update) reuses the view just loaded: one query less per item.
                new = self._save(current, replace(current.data, status=status).normalized())
                if new is not None:
                    changed.append(new)
                if progress:
                    progress(len(changed), len(todo))
        return changed

    def set_priority(self, item_id: int, priority: Priority) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, priority=priority))

    def set_tags(self, item_id: int, tags) -> ItemView | None:
        current = self._require(item_id)
        return self.update(item_id, replace(current.data, tags=tags))

    def delete(self, item_id: int) -> int:
        """Delete an item (closes its current version). Returns the revision number."""
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
            target = self._target(current.project_id)
            if current.rev_start == target.number:
                # Changed (or created) in this revision: drop that version; the one it
                # replaced is already closed by this revision.
                before = repo.get_at_revision(self.db, item_id, target.number - 1)
                repo.delete_version(self.db, current.version_id)
                line = None if before is None else f"{current.code} excluído: {before.data.title}"
            else:
                repo.close_version(self.db, current.version_id, target.number)
                line = f"{current.code} excluído: {current.data.title}"
            self._record(target, item_id, line)
            return target.number

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
