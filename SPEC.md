# Projeto: Giano — gerenciador TUI de melhorias de sistemas

Quero desenvolver o **Giano**, uma aplicação de terminal (TUI) para registrar e organizar
melhorias, discussões e ajustes dos sistemas que desenvolvo, separados por projeto.
Antes de escrever código, leia esta especificação inteira e me apresente um plano de
implementação em fases. Só comece depois da minha aprovação.

## Stack
- Python 3.12+, gerenciado com **uv** (pyproject.toml, entry point `giano`)
- **Textual** para a TUI
- **SQLite** via `sqlite3` da biblioteca padrão (sem ORM), com migrações simples
  controladas por uma tabela `schema_version`
- **markdown-it-py** + **Jinja2** + **Pygments** para gerar HTML
- **Typer** para os comandos de linha de comando
- **platformdirs** para a localização do banco (padrão: diretório de dados do usuário;
  pode ser sobrescrito pela variável `GIANO_DB`)
- **pytest** para os testes
- Deve funcionar em Windows, Linux e macOS
- Código e identificadores em inglês; textos da interface em português

## Conceitos

### Projetos (configuráveis)
Crio, edito e arquivo projetos na própria TUI. Cada projeto tem: sigla curta e única
(ex.: MERC, P2K), nome, descrição em Markdown (contexto do sistema, usado também na
exportação para o Claude) e o campo ativo/arquivado.

### Itens da lista de melhorias
Cada projeto tem a sua lista. Cada item tem:
- identificador estável por projeto: sigla + número sequencial (ex.: MERC-12)
- título
- tipo: Melhoria / Discussão / Ajuste
- prioridade: Crítica / Alta / Média / Baixa
- dificuldade: 1 a 5
- status: Aberto / Em análise / Aprovado / Em desenvolvimento / Concluído / Descartado
- tags livres (várias por item, com autocompletar das tags já existentes)
- corpo em **Markdown**, que é a única fonte de verdade. O HTML nunca é guardado no
  banco: é sempre gerado a partir do Markdown.

### Arquivo de itens
- Um item pode ser **arquivado manualmente** (nunca automaticamente): sai da lista normal
  e das exportações (HTML e Claude), mas continua guardado com todas as características que
  tinha no momento do arquivamento.
- Arquivar e restaurar (voltar para a lista normal) geram revisão, como qualquer alteração.
  O arquivamento é um campo da versão (`archived`), então a lista de qualquer revisão
  antiga continua reproduzível.
- Item arquivado é somente leitura: para editá-lo, restaure-o primeiro.
- Na TUI, `v` alterna entre a lista e o arquivo; `z` arquiva (ou, no arquivo, restaura).

### Adendos
- Quando um item volta do desenvolvimento com problema ou ajustes, crio um **adendo** em vez
  de reescrever o item: o texto original continua intacto.
- O adendo mantém o número do original com um sub-número: `MERC-33.1`, `MERC-33.2`… Pedir
  um adendo de um adendo cria o próximo adendo do original.
- O adendo é um item próprio, gerido de forma independente: título, tipo, prioridade,
  dificuldade, status, tags, corpo, revisões e arquivamento são dele. Posso concluir ou
  arquivar o original e deixar só o adendo na lista.
- Criar um adendo gera revisão ("MERC-33.1 criado (adendo de MERC-33): …"). Um item com
  adendos (mesmo arquivados) não pode ser excluído.
- Na exportação HTML posso escolher só o adendo (`--itens 33.1`); ele sai com a referência
  ao original (código, título, status) e o texto do original recolhido. Se o original
  também estiver no documento, os dois se ligam por links. Original arquivado aparece só
  como referência, sem o texto.
- Na TUI, `d` cria um adendo do item selecionado (começa como Ajuste, com prioridade,
  dificuldade e tags do original); no editor, `ctrl+o` mostra o original.

### Revisões da lista (requisito central)
- **Toda alteração na lista de um projeto gera uma nova revisão**: criar, editar,
  excluir ou mudar status, prioridade ou tags de qualquer item.
- Uma edição gera uma revisão apenas ao ser salva, nunca a cada tecla. Salvar sem
  mudança real não gera revisão.
- A numeração das revisões é sequencial por projeto (Rev. 1, 2, 3…). Cada revisão
  guarda data/hora e um resumo automático da mudança
  (ex.: "MERC-12: prioridade Alta → Média").
- Implemente com **versionamento temporal, só por inserção** (append-only):
  - `items` guarda a identidade estável (id, project_id, numero)
  - `item_versions` guarda o conteúdo, com `rev_inicio` e `rev_fim`
    (rev_fim NULL = versão vigente)
  - Uma alteração fecha a versão vigente (`rev_fim = N+1`) e insere a nova
    (`rev_inicio = N+1`). Excluir apenas fecha a versão vigente.
  - A lista na revisão N é composta pelas versões com
    `rev_inicio <= N AND (rev_fim IS NULL OR rev_fim > N)`
  - As tags ficam ligadas à versão do item, não ao item.
  - Cada alteração roda numa única transação.
- **Por padrão a TUI mostra sempre a última revisão.** Posso escolher uma revisão
  anterior, que aparece em modo somente leitura, com indicação clara no cabeçalho
  (ex.: "Rev. 9 de 14 — somente leitura").
- Quero ver as diferenças entre duas revisões: itens adicionados, removidos e alterados,
  com os campos que mudaram.

### Anotações rápidas (para reuniões)
- Captura rápida acessível de qualquer tela com um atalho global, e também pelo
  terminal com `giano nota "Nome da reunião" [--projeto SIGLA]`.
- Cada anotação tem: **data** (padrão hoje, editável), **nome** para identificação,
  projeto opcional e corpo em Markdown livre.
- Salvamento automático enquanto digito: não posso perder nada no meio de uma reunião.
- Status: Pendente / Processada / Arquivada.
- A partir de uma anotação, posso criar um ou mais itens de melhoria. O item guarda o
  vínculo com a anotação de origem.
- As anotações não fazem parte da lista versionada, portanto não geram revisões.

### Exportação para o Claude
Comando `giano export claude --projeto SIGLA [--desde AAAA-MM-DD] [--todas]`, que também
fica disponível na TUI. Gera um único arquivo .md e copia o conteúdo para a área de
transferência (na TUI use `App.copy_to_clipboard`). O arquivo é pensado para eu colar
numa conversa com o Claude e analisar as anotações à luz do estado do projeto:
1. Contexto do projeto (descrição em Markdown)
2. Lista atual de melhorias (última revisão), em formato compacto: identificador,
   título, tipo, prioridade, dificuldade, status, tags e corpo
3. Anotações selecionadas (por padrão, as pendentes), em ordem cronológica, cada uma
   com data e nome
4. Uma seção final de pedido de análise. O texto vem de um template editável nas
   configurações, com um padrão como: relacionar as anotações aos itens existentes,
   apontar duplicidades e conflitos, propor novos itens com tipo, prioridade e
   dificuldade sugeridos.

### Exportação HTML
- `giano export html --projeto SIGLA [--rev N]` (padrão: última revisão), também
  disponível na TUI para a revisão que estiver sendo exibida.
- Um arquivo HTML **autocontido** (CSS inline, sem dependências externas), legível e
  adequado para impressão, pensado para enviar a outras pessoas.
- Conteúdo:
  - cabeçalho com nome do projeto, número e data da revisão
  - sumário com links internos para os itens
  - itens agrupados por tipo, com etiquetas de prioridade, dificuldade, status e tags
  - corpo renderizado do Markdown, com destaque de código
  - seção opcional "Alterações nesta revisão"
- Nome do arquivo: `giano_<SIGLA>_rev<N>_<AAAA-MM-DD>.html`, numa pasta de exportação
  configurável.

## Interface (TUI)
- Barra lateral com os projetos; área principal com a tabela de itens (ordenável e
  filtrável por tipo, status, prioridade e tag); painel de pré-visualização Markdown
  do item selecionado.
- Editor de item: campos estruturados + TextArea com destaque de Markdown e alternância
  editar/visualizar.
- Tela de anotações rápidas com lista e editor.
- Busca textual nos itens e nas anotações (use SQLite FTS5).
- Atalhos sugeridos (todos visíveis no rodapé):
  - `n` novo item, `e` editar, `del` excluir (com confirmação)
  - `/` buscar, `f` filtros, `r` revisões, `x` exportar HTML
  - `a` anotações, `ctrl+n` anotação rápida em qualquer tela
  - `p` projetos, `q` sair

## Qualidade
- Testes pytest cobrindo sobretudo a lógica de revisões: reconstrução da lista em
  qualquer revisão, exclusão, edição sem mudança, diff entre revisões, além das
  exportações.
- Separe as camadas: banco/repositório, serviços (regras de revisão e exportação) e UI.
  A UI não acessa SQL diretamente.
- Crie um README curto e um CLAUDE.md com a estrutura do projeto e os comandos de
  desenvolvimento.
- Gere dados de exemplo com `giano demo` para eu testar rapidamente.
