# Giano para Windows (Tauri)

Versão desktop do Giano: núcleo em **Rust** (`crates/giano-core`) e interface em **Svelte 5 +
Tailwind** dentro de uma janela **Tauri 2** (WebView2, que já vem no Windows 11).

Usa **o mesmo banco** e o mesmo `config.json` da TUI em Python: as duas podem ser usadas lado a
lado (arquivo local com WAL, ou o mesmo banco no Turso). A TUI continua funcionando como antes.

## Pré-requisitos (uma vez)

```bash
winget install Rustlang.Rustup
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Node.js 20+ já basta para a interface.

## Comandos (dentro de `desktop/`)

```bash
npm install                      # dependências da interface
npm run tauri dev                # abre o app com recarga automática
npm run tauri build              # instalador em target/release/bundle (msi e nsis)
cargo test -p giano-core         # testes do núcleo (revisões, adendos, exportações...)
npm run check                    # checagem de tipos da interface
```

Para testar sem mexer nos seus dados, aponte para outro arquivo:
`$env:GIANO_DB="$env:TEMP\giano-teste.db"; npm run tauri dev` (PowerShell), e use
**Configurações → Dados de exemplo**.

## Estrutura

```
desktop/
  crates/giano-core/     domínio, banco (SQLite via rusqlite / Turso via Hrana HTTP),
                         serviços (revisões, anotações, busca FTS5, exportações) e testes
    templates/           export.html.j2 (minijinja; mesmo HTML da versão Python)
  src-tauri/             app Tauri: comandos finos que chamam o giano-core
  src/                   interface (Svelte 5): views/, lib/ (api, estado, componentes)
```

O núcleo segue as mesmas regras da versão Python (`SPEC.md`, `CLAUDE.md`): versões
append-only, uma revisão por mudança numa única transação, salvar sem mudança não gera
revisão, adendos, arquivo, exportações. As migrações são as mesmas (`MIGRATIONS`): ao mudar o
esquema, acrescente a nova versão nas duas listas, com o mesmo número.

## Atalhos

| Tela | Atalhos |
|---|---|
| Global | `Ctrl+K` buscar · `Ctrl+N` anotação rápida · `F5` atualizar |
| Itens | `N` novo · `E`/`Enter` editar · `D` adendo · `Del` excluir · `S` status · `Z` arquivar/restaurar · `V` lista/arquivo · `F` filtrar · `R` revisões · `L` última revisão · `X` HTML · `C` Claude · `Espaço`/`Ctrl+A`/`Shift+↑↓` seleção múltipla |
| Editor | `Ctrl+S` salvar · `Ctrl+T` escrever/visualizar · `Ctrl+O` ver o original (adendo) · `Esc` fechar |
| Anotações | `Ctrl+S` salvar agora · `Ctrl+O` criar item · `Ctrl+T` visualizar |
| Revisões | `↑↓` navegar · `Enter` abrir a lista · `M` marcar a base da comparação |
