# NarraHub — Estado corrente de engenharia

> Fonte da verdade sobre "onde estamos". Qualquer agente lê este arquivo antes de agir.
> Atualize-o ao fechar uma tarefa que mude versão, fase ou dívida conhecida.

Atualizado em: 2026-09-09

## Versão

| Item | Valor |
| --- | --- |
| Versão corrente | **0.10.0-beta.6** |
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
FASE 4 — Sync V2   (etapas 1 a 13 de 14 concluídas)
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

> **O ADR 0009 foi aceito em 2026-09-02**, na terceira revisão. A implementação segue a
> **ordem da seção 23 do ADR**: catorze etapas, uma PR e um gate por etapa. A ordem não é
> sugestão — cada etapa só pode ser provada depois da anterior, e nenhuma fecha apoiada no
> gate da seguinte.
>
> **Etapa 1:** schema 16, estruturas e invariantes de banco.
> **Etapa 2:** log de eventos, `seq` atômico e cadeia de revisões.
> **Etapa 2.5:** identidade Ed25519 local, privada fora do banco, evento assinado no nascimento.
> **Etapa 3:** outbox transacional — salvar capítulo grava dado e evento na mesma transação.
> **Etapa 4:** aplicação de evento recebido, com as quatro saídas de causalidade.
> **Etapa 5:** ordem, lacunas e cursor contíguo — pendente é o que está no log e não foi aplicado.
> **Etapa 6:** troca de vetores e store-and-forward — propagação transitiva provada com três aparelhos.
> **Etapa 7:** cadeia de confiança — roster, estado, coerência da chave e verificação Ed25519.
> **Etapa 8:** Noise `XX` e o vínculo entre a sessão e a identidade Ed25519. **Sem rede real ainda.**
> **Etapa 9:** pareamento por QR — convite aleatório, expirável e de uso único. **Sem câmera nem socket.**
> **Etapa 10:** PIN por SPAKE2 — sem dicionário offline, com limite de três tentativas.
> **Etapa 11:** tombstones causais, coleta só com prova, e as duas saídas do conjunto. **Schema 17.**
> **Revisão 11.1:** a prova de saída limpa passa a vir do aparelho que sai; `mudar_estado`
> deixa de existir; a divergência registra a operação de cada lado. **Schema 18.**
> **Etapa 12:** bootstrap por snapshot atômico — captura numa transação de leitura só, e
> semeadura construtiva que recusa qualquer receptor não virgem. **Schema 19.**
> **Etapa 13:** assets por `SHA-256` (ADR 0010). **Schema 20.** Blob store, catálogo das dez
> superfícies, backfill das dez, transformador HTML único com `lol_html`, as três barreiras de
> entrada, evento de attachment e bootstrap com manifesto de blobs verificados antes do seed.
> `attachments` deixou de ser `EtapaPosterior` e passa a viajar no bundle. O backfill roda na
> **fronteira de arranque**, entre as migrations e o primeiro consumo do acervo. Na revisão da
> PR, `exigir_blob_safe` foi alinhado ao ADR 0010: fonte externa ou desconhecida
> (`https://`, caminho do Windows, `file://`, `blob:`) **não passa em persistência nova**, e o
> legado continua preservado byte a byte com pendência registrada.
> **Etapa 14 — implementada, falta o gate físico.** O Sync V2 atravessa TCP real: enquadramento,
> PIN por SPAKE2 com `XXpsk0`, admissão direta, bootstrap pela rede com blobs conferidos por SHA
> e incremental nas duas direções, provado por gate E2E com dois bancos, duas identidades e dois
> blob stores. Sete comandos na fronteira e um cartão mínimo em Configurações. O alvo Android
> compila e o CI constrói o APK. Falta executar `docs/ETAPA_14_ROTEIRO_FISICO.md` em Windows e
> Android reais.
>
> **Etapa G — Sync V1 removido do runtime.** O Sync V2 é o único protocolo de sincronização
> alcançável: `src-tauri/src/sync.rs`, os quatro comandos (`sync_status/start/stop/connect`), a
> porta `SyncService`, os DTOs e os cartões antigos de Configurações saíram. A cobertura que a
> `NH-079` cobrava fechou nas etapas B1–B6 (gate de cobertura total na B6). As tabelas
> `sync_conflicts`, `sync_peers` e `devices` ficam no schema como **legado histórico, não
> utilizado em produção, preservado só para upgrade e auditoria** — sem `DROP`, sem migration nova.
> Gates: `database/legado_v1.rs` (G1, G7, G12 e o vigia pelo autorizador do SQLite),
> `sync_sessao::g_o_fluxo_do_v2_nao_le_nem_escreve_o_legado_do_v1` (G4–G6, G10, G11) e os
> testes `ETAPA G` de `tests/rust-core-contract.test.mjs`.
>
> **Etapa H — hardening final do Sync V2.** Três riscos fechados, com a mesma prioridade: perda
> silenciosa é proibida; conflito a mais, espera e fail closed são respostas aceitáveis.
> **H-R1**: a regra R2 ("nunca materializou aqui") passou a exigir prova positiva — história vazia
> com o evento-base pendente, ou história feita só de decisões registradas. Ausência não é prova, e
> um tombstone coletado por um GC futuro não ressuscita mais nada. Toda remoção de tombstone passa
> por `sync_repository::remover_tombstone`, com motivo declarado; o GC físico continua não existindo.
> **H-R2**: `other_rev` deixou de ser autoridade. Um efeito sobre agregado que não é participante só
> ganha a junção de dois pais quando este aparelho prova que ele pertence à ação original do
> conflito (mesma origem, mesmo `mutation_id`) e o **par inteiro** é a aresta daquela ação ou as
> cabeças das duas ações participantes — uma das pontas não basta (H28). A ação é identificada pelo
> `event_id` que a história local registra, não por revisão igual (H29). Sem prova, o efeito segue
> como evento comum: sequencial aplica, concorrente vira pergunta.
> **H-R3**: `legacy_recovery_items` (migration 29) inventaria o que o Sync V1 deixou pendente neste
> aparelho. O import roda no arranque, depois da conversão de mídia e antes de `Ready`; depois
> disso ninguém lê `sync_conflicts`. O escritor preserva (vira capítulo novo, que sincroniza) ou
> descarta com confirmação. Gates H1–H27 em `application/{hardening,legado}_testes.rs`.
>
> Reconciliação fina de capítulo por bloco depende da **NH-045** e não faz parte das 14
> etapas. O Sync V2 pode fechar com conflito seguro de capítulo inteiro.

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
| Sync V1 sem criptografia | **Removido do runtime** (etapa G); tabelas ficam como legado histórico |
| Sync V2 | **ADR 0009 `Accepted`.** Etapas 1–13 concluídas (**ADR 0010** fecha os assets); falta rede real, o gate de saída **NH-053** e a propagação da saída (**NH-058**, parcial) |
| Context Engine / IA | **Não iniciado** |
| Qualification harness | **Concluído.** Migration, backup, restore e rollback cobertos por `cargo test` no CI |
| Ciclo de atualização empacotado | **Concluído.** Roteiro, checklist de release e três execuções reais |
| Shell mobile | **ADR 0011 `Accepted`.** DesktopShell e MobileShell compartilham domínio e navegação, não a composição visual. Gates: `tests/mobile-shell.test.mjs` e Playwright (`test:mobile-e2e`, job Mobile na CI). Falta o roteiro físico (`docs/mobile/ROTEIRO_ANDROID.md`) |

## Versões e schema

Para escolher o par de versões de qualquer teste de upgrade, o que importa é cruzar
migration — não pegar a versão mais recente:

| Versão | Schema |
| --- | --- |
| 0.7.6 | 14 |
| 0.8.0 | 14 |
| 0.9.0 e 0.9.1 | 15 |
| 0.9.2 (publicada) | 15 |
| 0.10.0-beta.1 e beta.2 (pré-releases Android) | 20 — Sync V2 anterior ao `Hello`; o upgrade gira a época causal (E0-beta, `fixtures/beta2`) |
| `main` hoje | **29** — caixa de recuperação do legado (etapa H, H-R3): `legacy_recovery_items` |

Consequência prática, e ela **mudou** com a migration 16: a próxima versão publicada será a
primeira desde a 0.9.2 a carregar migration de verdade. O par `0.9.2 → próxima` deixa de ser
inerte e passa a exercitar o upgrade — inclusive a substituição do formato antigo de
`sync_events`, que nunca foi povoado mas existe no banco de todo usuário desde a v1.

O par `0.8.0 → 0.9.1` continua servindo para testar upgrade de duas migrations, e as duas já
estão publicadas com instalador e assinatura.

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

## O que a revisão 11.1 mudou de entendimento

A etapa 11 fechou verde e com gates. A revisão do autor achou três coisas que os gates não
pegavam, e todas são da mesma família: **o teste provava o caminho bonito e deixava o atalho
aberto ao lado.**

1. A saída limpa comparava `MAX(seq)` conhecido contra a confirmação de um peer. Quando os
   dois lados ignoram os mesmos eventos, a conta dá zero — e um aparelho com cinco alterações
   offline saía como "limpo". A prova agora vem de quem sai, autenticado, e não pode encolher
   o próprio passado.
2. `sync_trust::mudar_estado(conn, id, "retired")` escrevia o mesmo estado sem pré-condição
   nenhuma. Provar `aposentar` não vale nada enquanto o atalho continua exportado — a função
   foi removida, e um gate reprova se `estado: &str` voltar como parâmetro.
3. Três asserts do gate de exclusão concorrente eram tautologias
   (`assert_eq!(x, "".to_string().max(x.clone()))`). Passavam com o mecanismo removido.

A lição que vale para as etapas 12–14: **um gate só conta depois de ser visto reprovando**, e
uma mutação isolada é a única forma de ver isso.

## E o que a revisão 11.2 mudou

A 11.1 foi revisada pelo autor, que achou mais duas coisas — as duas da mesma família de novo:

1. **Estado de saída não era terminal.** A pré-condição só olhava `is_self`, e o `UPDATE` não
   tinha `AND state = 'active'`. `retired`/`clean` podia virar `retired`/`abandoned` ou
   `revoked`, apagando o registro de que a saída tinha tido prova. Nenhum gatilho pegava: as
   duas pontas são estados válidos, e o defeito estava na transição.
2. **O gate da NH-058 procurava `append_local_event` por regex.** Presença de identificador não
   é comportamento — e o vício reapareceu justamente no gate escrito para denunciá-lo. Agora
   ele abandona um aparelho de verdade e observa se o log cresceu.

O padrão que se repete nas três revisões: **o gate existia, passava, e provava outra coisa.**

## O que a etapa 13 encontrou fora dela

**As superfícies de documento guardam HTML, não JSON do Tiptap.** O ADR 0010 e o desenho
aprovado descrevem `chapters.content` como árvore JSON com nodes `image` de `attrs`. O
repositório faz `editor.getHTML()` na saída e `setContent(html)` na entrada, e
`normalizeIncoming` ainda envelopa texto puro legado em `<p>`. O que chega ao SQLite é
`<img src="data:image/png;base64,…">`. A premissa falsa é minha, e está registrada em
**NH-065**, **decidida** em 2026-09-10: o HTML continua a representação persistida, e a
transformação usa `lol_html`. O formato canônico é
`<img data-narrahub-blob="<64 hex>" data-mime-type="…">`.

**E uma continuação de linha perdida virou dezoito espaços na tela do escritor.** Oito
mensagens carregavam a assinatura, duas delas já na `main` desde a etapa 11. O defeito
compila, `cargo fmt` aceita, e nenhum teste de comportamento reclama — só o texto muda.
A causa era a ferramenta com que eu escrevia os arquivos, que consumia a barra invertida.
Fechado por `interface/writer_messages.rs`.

<!-- chapter-revisions:premissa-corrigida -->
**`chapter_revisions` não é tabela morta.** `docs/ARCHITECTURE_EVOLUTION_PLAN.md` afirmava que
ela "nunca teve escrita nenhuma, nem no frontend nem no Rust". A segunda metade é verdadeira, e
é o que fazia a primeira soar verificada — quem escreve é o gatilho `trg_chapter_revision`,
desde a migration 1. **Buscar escritores em código-fonte não encontra escritores em SQL.**
Documento corrigido, com gate behavioral em Rust e gate de documentação em JS.

**Dois gates meus passavam pelo motivo errado.** O da heurística de nome aprovaria qualquer
coisa se a heurística deixasse de casar — ganhou uma segunda metade que exige encontrar as seis
colunas conhecidas. O `coluna_de_referencia_nasce_vazia` contava zero linhas porque a fixture
não semeia aquelas tabelas, e foi removido com a nota do motivo no lugar.

## O que a etapa 12 encontrou fora dela

Dois defeitos que já estavam em `main`, achados pela revisão do autor enquanto o contrato do
bootstrap era fechado:

1. **`vetor_local()` inflava o progresso.** Para origem sem cursor, ela caía em `MAX(seq)` de
   `sync_events` sem filtro. Um evento estrangeiro chegando fora de ordem — guardado, não
   aplicado — fazia o aparelho anunciar progresso que não tinha. O peer parava de reenviar a
   lacuna e o que faltava não chegava nunca mais. **Roda em toda sessão da etapa 6**, não só no
   bootstrap.
2. **O baseline se re-semeava por `DELETE` + `INSERT`.** O gatilho da v16 só cobria `UPDATE`.

E um terceiro, este só possível a partir da etapa 12: o próximo `seq` local sai de
`sync_events`, não do cursor. Um receptor que reusasse uma identidade já conhecida pelo
conjunto começaria a escrever em `seq = 1` sobre coordenadas que já existem.

## Dívida arquitetural conhecida

- A tela de recuperação de schema (`NH-015`) **nunca foi vista rodando** — só testada. Para
  vê-la, aponte um perfil descartável para um banco de schema maior que o
  `LATEST_SCHEMA_VERSION` e rode `npm run desktop:qualification`.
- **Versões já publicadas continuam sem a tela de recuperação.** A 0.8.0 é imutável: quem
  voltar para ela seguirá com um app que não abre. O portão só protege downgrades feitos a
  partir da primeira versão que o contiver.

- ~~Sync V1 sem transporte criptografado, identidade, outbox nem tombstones~~ — **removido do
  runtime na etapa G**. Um conflito V1 que ficou aberto num banco publicado continua guardado em
  `sync_conflicts`, sem tela para resolvê-lo (o V1 nunca teve uma) e sem bloquear o pareamento.
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
