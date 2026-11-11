"""Locations and defaults."""

from __future__ import annotations

import os
from pathlib import Path

import platformdirs

APP_NAME = "giano"


def default_db_path() -> Path:
    """``GIANO_DB`` overrides the default location (the user's data directory)."""
    env = os.environ.get("GIANO_DB")
    if env:
        return Path(env).expanduser()
    return Path(platformdirs.user_data_dir(APP_NAME, appauthor=False)) / "giano.db"


def default_export_dir() -> Path:
    return Path(platformdirs.user_documents_dir()) / "Giano"


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
