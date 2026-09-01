# Fases 3 e 3.5 — Handoff

```text
Agente:  Claude
Data:    2026-09-01
Branch:  claude/NH-030-rust-core
Origem:  revisão arquitetural do autor, confirmada no código antes de agir
Status:  DONE — fecham as fases 3 e 3.5
```

## A revisão estava certa nos cinco pontos

Verifiquei cada um antes de mexer. Todos procedem, e dois são falhas que eu tinha declarado
resolvidas.

| Achado | Verificação |
| --- | --- |
| `invoke()` em 7 arquivos, não 1 | confirmado |
| o teste de contrato só procura `RustCoreService`, nunca `invoke(` | confirmado |
| legado é 1 comando, não "dois caminhos completos" | confirmado |
| `planning_save_card` é comando de domínio fora de `interface/tauri` | confirmado |
| `PROJECT_STATE` desatualizado | pior: citava uma branch já apagada |

## O legado era menor do que eu descrevi

`commands/` tinha **35 linhas**. Oito dos dez arquivos eram uma linha de comentário —
placeholders de quando o CRUD passava pelo `tauri-plugin-sql` no frontend. O único comando
registrado, `get_app_info`, **não era chamado por ninguém**.

Removi em vez de migrar: migrar código morto para um endereço melhor continua sendo código
morto. Se a informação de app voltar a ser necessária, ela nasce em `interface/tauri` como
comando de verdade.

Minha descrição de "dois caminhos completos até o banco" era alarmista, e o handoff anterior
já suspeitava disso sem ter ido conferir.

## O problema real era outro, e um gate contra `commands/` não o pegaria

`database/planning.rs` continha `planning_save_card`: `#[tauri::command]`, validação,
abertura de conexão própria, transação e SQL — 354 linhas contradizendo
`interface → application → domain → repository`.

Migrado inteiro:

```text
interface/tauri/planning_commands.rs   o comando
application/planning_service.rs        validação e orquestração da transação
infrastructure/sqlite/planning_repository.rs   o SQL
```

Os dois testes que o protegiam vieram junto e passam. **178 testes antes, 178 depois** — nada
se perdeu. E os dois estão no mapa de invariantes (4 e 10): mantiveram o nome, então o gate
das invariantes continua encontrando-os.

Uma decisão de desenho: `save_card` pega a conexão, `save_card_with` faz o trabalho sobre uma
conexão já obtida. É o que permite os testes continuarem rodando contra banco em memória —
a rede de segurança da operação existe desde antes da migração e não podia ser perdida nela.

## O gate, no formato mais forte

Não "sem `commands/`", e sim:

> Comando de domínio só nasce em `interface/tauri`.

Com exceções **nomeadas** para infraestrutura genuína — backup, recovery, health, réplica, IA,
share, sync, updater — cada uma com o motivo escrito, e um teste que cobra a existência do
motivo. Exceção sem justificativa é violação com permissão.

Duas mutações, duas reprovações: comando de domínio no service reprova, `commands/` recriado
reprova.

**O gate acusou a si mesmo primeiro.** A primeira versão marcou `planning_service.rs` e o
próprio arquivo do gate, porque os dois **citam** `#[tauri::command]` em comentário. Agora ele
olha linha que começa com o atributo, não texto solto.

## Fase 3.5 — a fronteira nativa

Está no [ADR 0008](../ADR/0008-fronteira-nativa-e-portas-de-plataforma.md) e no handoff da
PR #28. Resumo: duas portas em vez de uma, seis serviços movidos para `core/native/`,
`NativeWindowService` criado, e um gate que varre `src/app` atrás de `invoke()`, janela e
plugins.

A boa notícia da verificação: **ninguém chamava `invoke()` fora das portas.** A regra era
respeitada de fato; o que faltava era ser verificável.

## Validação executada

```text
cargo test                 ✅ 178 passed, 0 failed, 1 ignored
cargo fmt --check          ✅
npm run test:architecture  ✅ 45/45
4 mutações                 ✅ todas reprovam
```

## Próximo trabalho recomendado

`NH-040` — **ADR do Sync V2, antes de qualquer código.** O roadmap é explícito, e a revisão
reforçou: threat model primeiro. Contra quem estamos nos defendendo é outro dispositivo na
mesma rede capturando tráfego, se passando por peer ou fazendo replay — **não** alguém que já
desbloqueou a máquina.

E a decisão que já está registrada: **sem CRDT agora.** Outbox, operações idempotentes,
tombstones e conflitos explícitos resolvem os agregados. Texto de capítulo em edição
simultânea é o único candidato legítimo, e só no futuro, só naquele agregado.
