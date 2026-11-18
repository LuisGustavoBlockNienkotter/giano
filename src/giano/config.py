"""Locations, the per-computer configuration file and defaults.

Where the data lives (first match wins):

1. ``--db`` / ``GIANO_DB``: a local SQLite file (tests, demos, offline copies).
2. ``GIANO_TURSO_URL`` + ``GIANO_TURSO_TOKEN``: a Turso database.
3. ``turso_url`` + ``turso_token`` in ``config.json`` (``giano turso configurar``).
4. The local file in the user's data directory.

``config.json`` lives in the user's config directory (``GIANO_CONFIG_DIR`` overrides it)
and holds only what belongs to this computer: the Turso connection and the export folder.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path

import platformdirs

APP_NAME = "giano"
CONFIG_FILE = "config.json"

TURSO_URL = "turso_url"
TURSO_TOKEN = "turso_token"
EXPORT_DIR = "export_dir"


def default_db_path() -> Path:
    """``GIANO_DB`` overrides the default location (the user's data directory)."""
    env = os.environ.get("GIANO_DB")
    if env:
        return Path(env).expanduser()
    return Path(platformdirs.user_data_dir(APP_NAME, appauthor=False)) / "giano.db"


def default_export_dir() -> Path:
    return Path(platformdirs.user_documents_dir()) / "Giano"


# ---------------------------------------------------------------------------
# Per-computer configuration file
# ---------------------------------------------------------------------------


def config_path() -> Path:
    env = os.environ.get("GIANO_CONFIG_DIR")
    base = Path(env).expanduser() if env else Path(platformdirs.user_config_dir(APP_NAME, appauthor=False))
    return base / CONFIG_FILE


def load_config() -> dict[str, str]:
    path = config_path()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    except (OSError, ValueError) as exc:
        raise RuntimeError(f"Não foi possível ler a configuração {path}: {exc}") from exc
    return {k: v for k, v in data.items() if isinstance(v, str)} if isinstance(data, dict) else {}


def save_config(**changes: str | None) -> Path:
    """Update keys of ``config.json`` (``None`` removes the key)."""
    data = load_config()
    for key, value in changes.items():
        if value is None:
            data.pop(key, None)
        else:
            data[key] = value
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    try:
        os.chmod(tmp, 0o600)  # holds the Turso token
    except OSError:  # pragma: no cover - Windows ignores most modes
        pass
    os.replace(tmp, path)
    return path


# ---------------------------------------------------------------------------
# Database location
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class DbTarget:
    """A local SQLite file or a Turso URL (with its token)."""

    location: str
    auth_token: str | None = None

    @property
    def remote(self) -> bool:
        from giano.db.remote import is_remote_url

        return is_remote_url(self.location)


def normalize_turso_url(url: str) -> str:
    url = url.strip()
    if url and "://" not in url:
        url = "libsql://" + url
    return url


def turso_target() -> DbTarget | None:
    """The configured Turso database (environment first, then ``config.json``)."""
    url = os.environ.get("GIANO_TURSO_URL")
    if url:
        return DbTarget(normalize_turso_url(url), os.environ.get("GIANO_TURSO_TOKEN", ""))
    cfg = load_config()
    if cfg.get(TURSO_URL):
        return DbTarget(normalize_turso_url(cfg[TURSO_URL]), cfg.get(TURSO_TOKEN, ""))
    return None


def resolve_db_target(location: str | Path | DbTarget | None = None) -> DbTarget:
    if isinstance(location, DbTarget):
        return location
    if location is not None:
        return DbTarget(str(location))
    if os.environ.get("GIANO_DB"):
        return DbTarget(str(default_db_path()))
    return turso_target() or DbTarget(str(default_db_path()))


DEFAULT_CLAUDE_PROMPT = """\
Analise as anotações acima à luz do contexto e da lista atual de melhorias do projeto.

1. Relacione cada anotação aos itens existentes que ela afeta (cite os identificadores, ex.: {sigla}-12).
2. Aponte duplicidades: anotações que repetem itens já existentes ou que se repetem entre si.
3. Aponte conflitos: pontos das anotações que contradizem itens existentes, decisões ou o contexto do sistema.
4. Sugira atualizações para itens existentes (status, prioridade, descrição) quando as anotações justificarem.
5. Proponha novos itens para o que ainda não está na lista. Para cada um, informe: título, tipo
   (Melhoria / Discussão / Ajuste), prioridade sugerida (Crítica / Alta / Média / Baixa),
   dificuldade sugerida (1 a 5), tags e uma descrição curta em Markdown.

Responda em português, de forma objetiva, organizada nessas seções.
"""
