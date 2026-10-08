# NH-085 — Primeira fatia da implementação mobile V2

Agente: Codex. Data: 2026-09-30. Branch: `codex/nh-085-mobile-v2-configuracoes`.
Base: `mobile-shell @ 3e91b843c90093bcfea0a966a39ca839601a56ca`.

## Autorização e entrega

O usuário aprovou implementar a V2. Permanecem o relógio lateral, Claro/Escuro/Sistema e o
adiamento de mDNS/BLE/tela unificada. A branch contém a auditoria e as propostas anteriores,
preservadas, e a primeira fatia de implementação. TASKS foi atualizado em commit separado.

Mudanças em apresentação: settings com seletor mobile, cartões, controles e prévias; entidades
com grupos em linhas distintas; shell com margem para o alvo de 48px da alça. Os handlers e stores
continuam compartilhados com desktop. A UI de instalação local usa `localStatus.supported`.
Nenhuma migração, protocolo, comando Rust ou capacidade nova.

## Evidência e continuidade

Ver `docs/mobile/QUALIFICACAO_V2_FATIA_1.md`. A validação de UI combina inspeção de capturas,
geometria em quatro larguras e testes existentes. Instalação local no Windows foi apenas simulada
no navegador; nenhuma inferência de instalação real ou qualificação física nova.

O teste de peteleco foi estabilizado com timestamps CDP explícitos para representar 40ms,
independentemente da carga da máquina. O comportamento de produção permanece igual.

NH-085 não está DONE. Próxima fatia: biblioteca/criação, depois escrita e demais páginas,
segundo a prioridade da auditoria. Preparar uma nova beta para qualificação física quando o
conjunto visual estiver pronto; não reutilizar os PASS de QR da beta.13 como PASS da Fase 5.

O usuário já autorizou merge quando os CI passarem. Conferir base, SHA final e todos os checks
do PR antes de mesclar; a entrega desta fatia não encerra a fase nem promove para `main`.
