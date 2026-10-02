# Sync V2, etapa H — hardening final — Handoff

```text
Agente:  Claude
Data:    2026-09-23
Branch:  sync-h-hardening (base: mobile-shell @ 4c15aae)
Commit:  ver o PR da branch
Status:  REVIEW
```

## O que foi feito

A etapa que prova o que os casos extremos **não** conseguem. A prioridade acima de tudo:
**perda silenciosa é proibida**. Conflito explícito a mais, espera e fail closed são aceitáveis.

- **H-R1 — ausência não é prova.** A regra R2 ("nunca materializou aqui") passou a exigir prova
  positiva: história vazia com o evento-base exato pendente, ou história feita só de decisões
  registradas (conflito de nome de tag, exclusão bloqueada, decisão de grupo), ligadas à divergência
  que as registrou. Um tombstone coletado por um GC futuro deixa revisões materializadas que
  nenhuma decisão explica — e aí o efeito cai no classificador comum, que espera ou pergunta.
- **H-R1 — barreira contra o GC futuro.** Toda remoção de `sync_tombstones` passa por
  `sync_repository::remover_tombstone`, com três motivos declarados e nenhum de coleta. O GC físico
  **não** foi implementado.
- **H-R2 — `other_rev` não é autoridade.** Um efeito sobre agregado que não é participante só ganha
  a junção de dois pais quando ESTE aparelho prova que ele pertence à ação original do conflito
  (mesma origem, mesmo `mutation_id`) **e o par inteiro** é a aresta daquela ação (`{baseRev,
  newRev}` de um membro) ou as cabeças das duas ações participantes. Sem prova, o `other_rev` é
  descartado e o efeito segue como evento comum. Certificado inválido continua sendo recusa do
  grupo; falta de prova local, não — a diferença é o que protege o terceiro aparelho.
- **Correção pedida na revisão do PR #73:** a primeira versão provava "base ∈ ação **ou** other ∈
  ação". Isso deixava passar `{X1, X2}` — X1 da ação, X2 produzido depois pelo receptor —, e a R1
  sobrescrevia o X2. Agora `membros_da_acao_de` carrega a aresta inteira de cada membro, e o par é
  provado por completo (gate H28, mutação HM9).
- **H-R3 — caixa de recuperação do legado.** Migration 29 cria `legacy_recovery_items` (local, não
  causal). O import roda no arranque, depois da conversão de mídia e antes de `Ready`; é idempotente
  e nunca reabre decisão. A tela `/settings/recuperacao-sync-antigo` deixa o escritor preservar (vira
  capítulo novo por `Mutacao` normal, que sincroniza) ou descartar com confirmação. O aviso em
  Configurações não pode ser silenciado enquanto houver pendência.

## Decisões

- **Nada de formato novo.** `FORMATO_CANONICO_ATUAL` continua 2, protocolo 1. A H-R2 foi resolvida
  com validação mais estrita no receptor, não com evidência nova no certificado.
- **O importador é a única leitura nova de `sync_conflicts`**, e acontece antes de `Ready`. Ela está
  registrada na allowlist do gate G12 com esse motivo, e o H27 prova que depois de `Ready` ninguém
  toca a tabela — o autorizador do SQLite da etapa G continua vigiando.
- **Preservar é uma transação só**: criar o capítulo e marcar `preserved` acontecem juntos, ou nada
  acontece. Repetir depois do sucesso é recusado como "já resolvida".
- **Substituir o capítulo atual não é oferecido**: não existe causalidade V2 para uma decisão
  histórica.

## Descobertas

- O gate G12 da etapa G pegou, de verdade, a leitura nova de `sync_conflicts` do importador — ele
  fez exatamente o trabalho para o qual foi escrito, e a exceção teve de ser declarada à mão.
- O lote de mutações estourou o tempo no meio e deixou uma mutação aplicada no código. Foi por isso
  que o tempo de cada rodada virou filtro estreito, e o script passou a imprimir a conferência do
  `git status` depois de cada restauração.

## O que a segunda revisão do PR #73 pegou

Duas coisas, e as duas eram reais:

1. **A ação era identificada por revisão igual.** `new_rev` é determinístico e `sync_events` não tem
   `UNIQUE` por revisão, então a mesma revisão pode estar em duas mutações — uma com o auxiliar X,
   outra sem. Agora a âncora é o `event_id` da história local (H29, mutação HM10).
2. **O H13 era vácuo**, e por três motivos empilhados: ele conferia B depois de entregar em A; o
   capítulo alheio tinha conflito próprio, o que fazia o grupo ser recusado antes do auxiliar; a seq
   forjada caía sobre uma ocupada, e o receptor devolvia `JaAplicado`; e o payload forjado era lido
   do emissor **depois** das entregas, quando já era o estado do próprio receptor. Corrigidos os
   quatro, a HM2 (auxiliar sempre autorizado) passou a derrubar H8, H9, H12, H13, H14, H28 e H29.

Ficou uma lição para os gates de adulteração: além de afirmar o que não pode acontecer, eles têm de
provar que o evento forjado **chegou a ser processado** — hoje isso é `o_forjado_foi_processado`.

## O que a primeira revisão do PR #73 pegou

A prova do par auxiliar era fraca: bastava uma das pontas pertencer à ação. O ataque é uma ação
legítima `X0 → X1`, o receptor andando para `X2`, e um certificado dizendo `base = X1, other = X2`.
Corrigido com a prova do par inteiro, mais o gate H28 (com `chapter_position`, um auxiliar real) e a
mutação HM9, que reverte a prova e derruba exatamente o H28. Ao escrever o H28 eu mesmo errei antes:
reassinei a decisão com a chave do aparelho errado, e o receptor descartava os eventos — o gate
passava sem provar nada. A instrumentação mostrou o silêncio, e a correção fez a HM9 morder.

## Dívidas que ficam

- O GC físico de tombstones continua não existindo (por decisão desta etapa).
- Um terceiro aparelho que recebe o lado perdedor **depois** da decisão abre uma pergunta em vez de
  fechar sozinho: o certificado nomeia o participante, mas o índice local ainda não usa isso para
  superar a divergência. É segurança em cima de conveniência, e está registrado.
- A tela de recuperação não oferece exportar para arquivo; preservar como capítulo é a única saída
  que mantém a versão antiga viva.

## Validação executada

Ver o PR: gates H1–H27, mutações HM1–HM8, suítes e CI.
