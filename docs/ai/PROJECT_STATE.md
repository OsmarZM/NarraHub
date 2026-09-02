# NarraHub — Estado corrente de engenharia

> Fonte da verdade sobre "onde estamos". Qualquer agente lê este arquivo antes de agir.
> Atualize-o ao fechar uma tarefa que mude versão, fase ou dívida conhecida.

Atualizado em: 2026-09-01

## Versão

| Item | Valor |
| --- | --- |
| Versão corrente | **0.9.2** |
| Última tag publicada | `app-v0.9.2`, em 2026-09-01 |
| `origin/main` | 0.9.2 — canônica e **default** do repositório |
| Manifests, README e CHANGELOG | 0.9.2, sob teste no CI |

A linha "Versão corrente" acima é lida por `scripts/validate-release-version.mjs`: se ela
divergir dos manifests, o CI reprova. Este arquivo é a memória compartilhada de três agentes,
e memória compartilhada desatualizada é pior que memória nenhuma — os três raciocinam em cima
do erro ao mesmo tempo, com confiança.

## Branch canônica

`main`, canônica e default do repositório desde 2026-08-31.

- Protegida: push direto é recusado, promoção só por Pull Request.
- Branch nova nasce de `main` e volta para `main` por PR.
- As branches paralelas (`feat/native-app-foundation` e a de trabalho antiga) **foram
  apagadas**; todo o conteúdo delas está na `main`.

Ao verificar o estado das branches, compare sempre contra `origin/main` depois de
`git fetch` — a `main` local pode estar atrasada e dar um diagnóstico errado. Foi assim que
um diagnóstico de "fast-forward" saiu errado nesta sessão.

## Verificação pendente da 0.9.2

Publicada com a tabela 2.1 do checklist **dispensada por decisão registrada** — ver
`docs/releases/0.9.2.md`. Continua sem verificação visual:

1. o updater de um NarraHub 0.9.1 instalado detectando e aplicando a 0.9.2;
2. a tela de recuperação por schema incompatível;
3. a arte nova do tema claro.

A partir da 0.9.3 a tabela 2.1 é obrigatória.

## O que existe fora do repositório

A pasta que contém este repositório guarda um **andaime de 2026-08-20**, substituído por
inteiro pelo NarraHub atual:

```text
Projetos MVP/NarraHub/          não é repositório git
├── narrahub-app/               ← a raiz do repositório é aqui
├── angular-src/                66 ocorrências de SQL em serviços hoje proibidos
├── rust-src/commands/          byte a byte igual ao diretório removido na Fase 3
├── design-system/              CSS anterior ao styles.css
└── lançadores .bat/.ps1        conveniência local; duplicam npm start e desktop:dev
```

**Decisão de 2026-09-01: nada disso é versionado.** Commitá-lo devolveria ao repositório o
código que várias fases trabalharam para remover — e num lugar onde os gates não olham, já
que eles varrem `src/`. Num projeto com três agentes fazendo `grep`, código morto com nome
vivo é pior que código morto apagado: o próximo que procurar `universe.service.ts` acharia o
defunto.

Os arquivos continuam no disco do autor. O que o Git precisa preservar — a história do código
real — ele já preserva.

O gate `o andaime superado não volta para o repositório` reprova se `angular-src/`,
`rust-src/` ou `design-system/` aparecerem na raiz.

## Fase ativa

```text
FASE 4 — Sync V2
```

As fases **3 e 3.5 fecharam em 2026-09-01**, com gates executáveis:

| Gate | Reprova quando |
| --- | --- |
| `comando de domínio só nasce em interface/tauri` | um `#[tauri::command]` de domínio aparece fora do lugar |
| `o diretório commands legado não volta` | o caminho antigo é recriado |
| `só as portas nativas falam com o Tauri` | `invoke()`, janela ou plugin fora de `core/native` |

O legado era menor do que o roadmap dizia — 35 linhas, oito arquivos de placeholder — e o
problema real era outro: um comando de domínio em `database/planning.rs`. Um gate contra o
diretório não o pegaria; o gate contra **colocação** pega.

## Antes de escrever qualquer código do Sync V2

> **O ADR existe e está `Proposed`.** `docs/ADR/0009-sync-v2.md`, terceira revisão, aguardando
> decisão humana. **Nenhum código de sync deve ser escrito enquanto ele não for `Accepted`** —
> as três revisões mudaram premissa de produto, modelo de persistência e esquema de pairing,
> e implementar em cima de um documento em revisão é construir para jogar fora.

O roadmap é explícito: **ADR e threat model primeiro**. O que precisa estar decidido antes:

1. contra quem estamos nos defendendo — outro dispositivo na mesma rede capturando tráfego,
   se passando por peer ou fazendo replay; **não** alguém que já desbloqueou a máquina;
2. transporte: Noise Protocol ou TLS com certificate pinning;
3. a matriz de conflitos, por agregado.

E a decisão que já está tomada e vale registrar: **sem CRDT agora.** Outbox, operações
idempotentes, tombstones e conflitos explícitos resolvem os agregados. Texto de capítulo em
edição simultânea é o único candidato, e só no futuro, só naquele agregado.

A mudança de fundo é `replicação de estado inteiro → replicação incremental de mudanças`.

## Status arquitetural

| Área | Status |
| --- | --- |
| SQL no frontend | **Eliminado** — proibido por `tests/frontend-boundaries.test.mjs` |
| Migração de Router | **Concluída** |
| Rust Application Core | **Concluído.** Comando de domínio só em `interface/tauri`, com gate |
| Validador de versão | Roda no CI comum; cobre os 3 manifests + README + CHANGELOG |
| CI | `ci.yml` cobre Angular + Rust em PR e push |
| `WorkspaceLayout` | **Resolvido** na Fase 2, com gate executável |
| `commands/` legado | **Removido** na Fase 3 |
| Fronteira nativa do frontend | **Formalizada** — ADR 0008 |
| Sync V1 sem criptografia | **Foco atual** — Fase 4 |
| Sync V2 | **ADR 0009 `Proposed`** (3ª revisão) — código não iniciado, e não deve iniciar antes do `Accepted` |
| Context Engine / IA | **Não iniciado** |
| Qualification harness | **Concluído.** Migration, backup, restore e rollback cobertos por `cargo test` no CI |
| Ciclo de atualização empacotado | **Concluído.** Roteiro, checklist de release e três execuções reais |

## Versões e schema

Para escolher o par de versões de qualquer teste de upgrade, o que importa é cruzar
migration — não pegar a versão mais recente:

| Versão | Schema |
| --- | --- |
| 0.7.6 | 14 |
| 0.8.0 | 14 |
| 0.9.0 e 0.9.1 | 15 |
| `main` hoje | 15 |

Consequência prática: **uma 0.9.2 publicada hoje não exercitaria migration nenhuma** num
upgrade a partir da 0.9.1. O par útil hoje é `0.8.0 → 0.9.1`, e as duas já estão publicadas
com instalador e assinatura.

## Ambiente de desenvolvimento

`TMP`/`TEMP` desta máquina apontam para o `C:`, que está com 87% de uso. Uma
recompilação completa das dependências Rust derruba o `rustc` com
`STATUS_STACK_BUFFER_OVERRUN` em crates de terceiros — erro que parece bug de toolchain
e é falta de espaço. Rode cargo com os temporários no `D:`:

```bash
TMP='D:\DevTools\NarraHubTmp' TEMP='D:\DevTools\NarraHubTmp' cargo test --manifest-path src-tauri/Cargo.toml
```

O CI (Ubuntu) nunca reproduziu isso. Crash estranho de compilador aqui: suspeitar de
disco antes de suspeitar do código.

## Dívida arquitetural conhecida

- A tela de recuperação de schema (`NH-015`) **nunca foi vista rodando** — só testada. Para
  vê-la, aponte um perfil descartável para um banco de schema maior que o
  `LATEST_SCHEMA_VERSION` e rode `npm run desktop:qualification`.
- **Versões já publicadas continuam sem a tela de recuperação.** A 0.8.0 é imutável: quem
  voltar para ela seguirá com um app que não abre. O portão só protege downgrades feitos a
  partir da primeira versão que o contiver.

- Sync V1 não tem transporte criptografado, identidade de dispositivo, outbox nem
  tombstones, e usa `updated_at` para decidir concorrência. É o escopo da Fase 4, e a
  primeira tarefa é o ADR — não código.
- Sem teste de tokens de design — foi a causa do bug 0.9.0/0.9.1 (`var(--nh-glass-panel)`
  usado sem definição). Checagem ad hoc em 2026-08-31: 34 tokens definidos, 22 usados sem
  valor de reserva, **zero** usados sem definição. O estado hoje está são; nada impede a
  regressão de voltar. É a `NH-050`.
- O tema claro ganhou arte de fundo própria na PR #9 e **ainda não foi visto rodando** — o
  app precisa do runtime Tauri. Vale olhar com `npm run desktop:dev` antes de qualquer
  release.
- `public/assets/narrahub-logo-full.png` (1 MB) ficou sem referência depois da PR #9.
- Não existe escala compartilhada de breakpoints: 12 valores diferentes e 7 arquivos sem
  nenhuma media query (`NH-052`, Fase 5). O defeito de conteúdo inalcançável em 1366×768
  (`NH-051`) era outra coisa e já foi corrigido.

## Não trabalhar ainda

```text
Context Engine / embeddings
decomposição de features (Planning, Writing, Entities)
design system hardening e escala de breakpoints
colaboração em tempo real / CRDT
```

O Sync V2 **saiu desta lista**: ele é a fase ativa. Mas há uma ordem dentro dele que continua
valendo — **o ADR vem antes do código**, com threat model, causalidade e matriz de conflitos
decididos primeiro.

O que segue bloqueado está bloqueado pela ordem do roadmap, não por falta de rede: a Fase 1
fechou, e mudança nova já é provada contra migration, backup e restauração automaticamente.
