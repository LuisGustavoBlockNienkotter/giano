# Giano

Gerenciador de melhorias, discussões e ajustes de sistemas, no terminal (TUI).
Cada projeto tem sua lista de itens, **versionada por revisões**: toda alteração gera uma
nova revisão, e qualquer revisão anterior pode ser consultada, comparada e exportada.

- Lista por projeto com identificadores estáveis (`MERC-12`), tipo, prioridade,
  dificuldade, status, tags e corpo em Markdown
- Histórico completo (Rev. 1, 2, 3…), modo somente leitura para revisões antigas e
  diferenças entre quaisquer duas revisões
- Anotações rápidas de reuniões, com salvamento automático, de qualquer tela (`ctrl+n`)
  ou do terminal (`giano nota`)
- Exportação para o Claude (Markdown com contexto, lista atual e anotações pendentes)
- Exportação HTML autocontida, pronta para enviar ou imprimir
- Busca textual (SQLite FTS5) em itens e anotações

## Instalação

Requer [uv](https://docs.astral.sh/uv/) e Python 3.12+ (o uv instala o Python se precisar).

```bash
git clone https://github.com/LuisGustavoBlockNienkotter/giano.git
cd giano
uv sync
uv run giano demo     # dados de exemplo (projetos LOJA e RH)
uv run giano          # abre a interface
```

Para ter o comando `giano` disponível em qualquer pasta:

```bash
uv tool install . --python 3.12
uv tool update-shell   # se o uv avisar que a pasta de ferramentas não está no PATH
```

(O `--python 3.12` é necessário porque o `uv tool` ignora o `.python-version` do projeto, e o
`tree-sitter`, usado no destaque de Markdown, ainda não tem pacote pronto para o Python mais
recente no Windows.)

O banco fica na pasta de dados do usuário (ex.: `%LOCALAPPDATA%\giano\giano.db` no
Windows, `~/.local/share/giano/giano.db` no Linux). Use `GIANO_DB=/caminho/arquivo.db`
ou `giano --db arquivo.db` para usar outro.

## Atalhos da interface

| Tecla | Ação | Tecla | Ação |
|---|---|---|---|
| `n` | novo item | `r` | revisões (enter: ver; `d`: diferenças; `m`: marcar base) |
| `e` / enter | editar item | `l` | voltar à última revisão |
| `del` | excluir (com confirmação) | `x` | exportar HTML da revisão exibida |
| `s` | mudar status | `c` | exportar para o Claude |
| `/` | buscar | `a` | anotações |
| `f` | filtros | `ctrl+n` | anotação rápida (qualquer tela) |
| `o` | ordenar (ou clique no cabeçalho) | `p` | projetos |
| `g` | configurações | `q` | sair |

No editor de item: `ctrl+s` salva, `ctrl+t` alterna editar/visualizar, `esc` cancela.
Nas anotações: `ctrl+o` cria um item a partir da anotação (usando o texto selecionado).

## Linha de comando

```bash
giano nota "Reunião com o comercial" --projeto MERC   # abre o editor de anotação
giano nota "Ideia" --texto "corpo da anotação"         # cria sem abrir o editor
giano export claude --projeto MERC [--desde 2026-01-01] [--todas]
giano export html --projeto MERC [--rev 9] [--sem-alteracoes]
giano projetos | giano revisoes -p MERC | giano diff -p MERC 3 [9]
giano config [--pasta-exportacao PASTA]
giano demo
```

## Desenvolvimento

```bash
uv sync
uv run python -m pytest
```

Veja [CLAUDE.md](CLAUDE.md) para a arquitetura e as convenções.
