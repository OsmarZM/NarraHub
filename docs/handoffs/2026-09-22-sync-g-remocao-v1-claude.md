# Sync V2, etapa G — remoção definitiva do Sync V1 — Handoff

```text
Agente:  Claude
Data:    2026-09-22
Branch:  sync-g-remocao-v1 (base: mobile-shell @ 0da51d6)
Commit:  ver o PR da branch
Status:  REVIEW
```

## O que foi feito

**Sync V1 = removido do runtime. Sync V2 = único protocolo alcançável.**

- **Backend:** saíram `src-tauri/src/sync.rs`, os comandos `sync_status/start/stop/connect` e o
  `SyncState`.
  - A captura do bootstrap deixou de ler `sync_conflicts`, e `FalhaDeCaptura::ConflitoV1Aberto` saiu.
  - No catálogo do bootstrap, `sync_conflicts` virou legado local (`LocalNaoTransferida`), como
    `sync_peers` e `devices`.
- **Frontend:** saíram `SyncService`, `SyncServerStatus`/`SyncResult`, o estado e as travas V1↔V2
  do `SettingsStore` e os dois cartões antigos de Configurações.
  - O cartão do V2 perdeu o "(teste de campo)" e o aviso "não use junto com a antiga".
  - A trava de restaurar backup passou a olhar a escuta do V2.
  - `/settings/conflitos` e o aviso E0 ficaram intactos.
- **Schema:** nenhuma migration. As três tabelas do V1 ficam como legado histórico, catalogadas em
  `src-tauri/src/database/legado_v1.rs`.

## Decisões

- **Sem `DROP`:**
  - `sync_conflicts` guarda a única cópia da versão perdedora de um conflito V1.
  - `sync_peers` e `devices` nunca tiveram escritor.
  - Uma migration só para apagar bytes históricos não se paga.
- **Conflito V1 aberto não bloqueia mais o bootstrap.** O V1 nunca teve como resolvê-lo, e sem o V1
  a trava viraria um aparelho que nunca mais doa. A linha fica intacta no doador.
- **A conversão de mídia do ADR 0010 sobre `sync_conflicts` fica.** É migração de banco antigo e
  roda no arranque, antes de `Ready`. Tirá-la deixaria base64 inline no banco e mexeria num
  catálogo fechado.
- **G10/G11 são provados em runtime, não só por texto.** O autorizador do SQLite (feature `hooks`
  do rusqlite, só em `dev-dependencies`) vigia toda conexão aberta por `apply_pragmas` sobre os
  bancos armados, inclusive na thread que atende o outro aparelho.
- **A condição antiga da NH-077** ("o V1 sai quando o gate físico fechar") foi superada pela
  autorização do autor para a etapa G. O roteiro físico continua valendo para o V2.

## Descobertas

- A `NH-079` ("só 2 de ~47 escritas geram evento") estava desatualizada: B1–B6 fecharam a cobertura,
  com gate de cobertura total na B6. Ela foi marcada `DONE`.
- Um `\b` dentro de template literal num gate JS virava backspace. Foi a mutação mJ1 que achou;
  corrigido para `\\b`.

## Dívidas que ficam

- `delete_universe` continua recusando sempre. É anterior à G, e o V1 também não o oferecia.
- Um conflito V1 que ficou aberto num banco publicado fica guardado sem tela. Se algum dia valer
  mostrá-lo, é leitura de auditoria, não resolução.
- O roteiro físico da etapa 14 (Windows ↔ Android reais) continua pendente.

## Validação executada

Ver o PR: suíte inteira, mutações, CI.
