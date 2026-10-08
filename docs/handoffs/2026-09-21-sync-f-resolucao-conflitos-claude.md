# Sync V2, etapa F — resolução de conflito como fato causal — Handoff

```text
Agente:  Claude
Data:    2026-09-21
Branch:  sync-f-resolucao-conflitos (base: mobile-shell)
Commit:  ver o PR da branch
Status:  REVIEW
```

## O que foi feito

- **Contrato**: resolver um conflito é uma `Mutacao` normal que sai como grupo atômico de kind
  `resolution`. O membro 0 é o certificado `conflict_resolution/<conflictKey>` (payload canônico sem
  timestamp nem aparelho, com `results[]` ordenado) e os membros 1..N são os efeitos. Detalhes em
  `docs/sync/MATRIZ_COBERTURA_NH079.md` §5.1.
- **Recepção**: `sync_codec::resolucao::validar_grupo` confere o certificado inteiro antes do
  SAVEPOINT. Só depois disso os efeitos aplicam como sequenciais de dois pais
  (`sync_apply::apply_efeito_de_resolucao`, com as regras R1 e R2). Qualquer falha recusa o grupo
  com zero materialização.
- **Resolvedor**: `application/resolucao_divergencia.rs` expõe `resolver_conflito(db, identidade, key, &Acao)`
  e `resolver_automaticas`, que a sessão chama depois da drenagem. Cobre upsert×upsert,
  upsert×delete, delete×delete (automático), grupo×grupo, `parent_deletion_blocked` e
  `tag_name_conflict` (renomear e mesclar). Decisões diferentes viram `concurrent` sobre `cr/K`.
- **Migration 28**: `conflict_resolutions` e `sync_divergences.resolution_rev`. Entra no bundle e
  tem a fixture nativa `fixtures/schema28_native.sql`. `FORMATO_CANONICO_ATUAL = VERSAO_DA_ADOCAO = 2`,
  e o protocolo continua 1.
- **Panorama**: conta as `sync_divergences` abertas. O V1 não entra mais.
- **Comandos e tela**: `sync_conflitos_listar`, `sync_conflito_inspecionar`, `sync_conflito_resolver`
  e `sync_v2_aviso_de_epoca`, com DTOs em `application/conflitos.rs`. A tela fica em
  `/settings/conflitos` (feature `conflicts`, porta `core/native/sync-conflicts.service.ts`). O
  cartão do Sync V2 em Configurações mostra "X conflitos precisam de atenção" e o aviso E0.

## Decisões

- Não existe kind `resolution_conflict`: a decisão concorrente é `concurrent` sobre `cr/K`.
- Antes de gravar os efeitos, `declarar()` fecha o índice local do conflito. Sem isso, o preflight
  de exclusão recusaria a própria resolução.
- Pergunta obsoleta (a revisão remota já tem sucessor no histórico) é fechada só no índice local,
  sem virar fato causal.
- O aviso E0 some por época (`localStorage`, chave `iniciadaEm`). Ele só mostra a contagem de
  divergências beta arquivadas e nunca as reativa.

## Limitações conhecidas

- A regra de par para efeitos irmãos/meta é de conhecimento genérico, não amarrada à chave.
- R2 poderia ressuscitar um agregado cujo tombstone o GC já podou (caso de borda).
- A superação de pergunta obsoleta só cobre sucessão remota.
- "Manter local" num conflito de grupo exclui criações que só existem no outro lado.
- Mesclar tags exige que os donos das marcações existam localmente.

## Validação executada

Ver o PR (números da suíte, mutações e CI).
