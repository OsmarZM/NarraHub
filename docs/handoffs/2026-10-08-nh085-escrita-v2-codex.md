# NH-085 — Escrita e ferramentas V2 / beta.17

## Objetivo e estado

Usuário aprovou a próxima fatia de escrita após confirmar restauração de textos e imagens. Data: 2026-10-08. Branch `codex/nh085-escrita-v2`, base `mobile-shell` em `968911a7df35a4c32ac5652b45ba64fac9c8463a`.

Reutilizado o worktree `narrahub-release-beta16`. O checkout original `narrahub-app` e suas alterações locais foram preservados. A proposta anterior dos três arquivos de editor foi reaplicada sobre a base atual e corrigida conforme reprodução. TASKS foi atualizado em commit separado `74821ac`.

## Mudanças

Barra compacta com seis controles e ferramentas completas recolhíveis; superfícies opacas e tipografia mobile; seleção preservada ao usar botões; estilo sincronizado; assistentes contidos no viewport. Reserva de espaço do relógio restrita à barra, sem faixa global. Desktop e rotas/stores/gateway mantidos. Versão beta.17 preparada nos manifests, locks e notas, sem dependência nova ou mudança de backend.

## Evidência e limites

Ver [qualificação da fatia 3](../mobile/QUALIFICACAO_V2_FATIA_3.md). Build final PASS, arquitetura 105 PASS, Android contrato 7 PASS, versão consistente. Geometria mobile 18 PASS, desktop 2 PASS, interação 6 PASS, regressão escrita 8 PASS/2 não aplicáveis. Browser usa fixtures; autosave prova a chamada ao gateway, não gravação SQLite. Sem PASS físico de escrita/voz/IA. NH-085 e Fase 5 continuam IN_PROGRESS.

## Entrega

Criar PR para `mobile-shell`; exigir os quatro checks SUCCESS no head exato. Merge e publicação beta já autorizados pelo usuário. Executar workflow oficial uma única vez no head validado, verificar tag/assets/checksum/assinatura e compatibilidade de certificado com beta.16. Publicação não implica instalação física. Demais páginas e M15 ficam pendentes.
