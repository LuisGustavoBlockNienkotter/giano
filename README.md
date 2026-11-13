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
- Arquivo de itens: tire da lista e das exportações o que não quer mais ver, sem perder nada

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
| `del` | excluir (com confirmação) | `x` | exportar HTML da revisão exibida (escolhendo os itens) |
| `s` | mudar status | `c` | exportar para o Claude |
| `z` | arquivar item (no arquivo: restaurar) | `v` | alternar lista / arquivo |
| `/` | buscar | `a` | anotações |
| `f` | filtros (dificuldade aceita faixas: `1,3-5`; por padrão esconde concluídos e descartados) | `ctrl+n` | anotação rápida (qualquer tela) |
| `o` | ordenar (ou clique no cabeçalho) | `p` | projetos |
| `g` | configurações | `q` | sair |
| `F5` | atualizar os dados (em qualquer tela) | `ctrl+p` | paleta de comandos |

`F5` recarrega o que foi alterado fora desta janela (outro Giano aberto, `giano nota`, um
script). O que você está editando nunca é descartado: se o item ou a anotação foi alterado
fora e você tem alterações não salvas, o Giano avisa e pede confirmação antes de substituir.

**Arquivo**: itens que você não quer mais ver (ex.: concluídos há tempos) podem ser
arquivados com `z`. Eles saem da lista normal e das exportações (HTML e Claude), mas ficam
guardados com tudo o que tinham; `v` mostra o arquivo, onde `z` devolve o item à lista.
Arquivar e restaurar geram revisão, como qualquer alteração. Item arquivado é somente
leitura até ser restaurado.

No editor de item: `ctrl+s` salva, `ctrl+t` alterna editar/visualizar, `esc` cancela.
Nas anotações: `ctrl+o` cria um item a partir da anotação (usando o texto selecionado).

## Linha de comando

```bash
giano nota "Reunião com o comercial" --projeto MERC   # abre o editor de anotação
giano nota "Ideia" --texto "corpo da anotação"         # cria sem abrir o editor
giano export claude --projeto MERC [--desde 2026-01-01] [--todas]
giano export html --projeto MERC [--rev 9] [--itens 1,3-5] [--sem-alteracoes]
giano projetos | giano revisoes -p MERC | giano diff -p MERC 3 [9]
giano itens -p MERC [--arquivados]
giano item novo "Título" -p MERC [--tipo ajuste] [--prioridade alta] [-d 2] [--status aprovado] \n    [--tags "a, b"] [--corpo "texto" | --corpo - < corpo.md]
giano item ver MERC-12
giano item editar MERC-12 [--status concluido] [--prioridade baixa] [...]   # só o que for informado
giano item arquivar MERC-12 | giano item restaurar MERC-12
giano config [--pasta-exportacao PASTA]
giano demo
```

Os comandos `giano item` não abrem a interface, então servem para scripts e para o Claude
registrar itens a partir de qualquer pasta. Tipo, prioridade e status aceitam o nome em
português sem acento (`em-analise`, `critica`) ou o valor interno (`analysis`).

## Desenvolvimento

```bash
uv sync
uv run python -m pytest
```

Veja [CLAUDE.md](CLAUDE.md) para a arquitetura e as convenções.
