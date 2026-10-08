# Handoff — NH-085 / biblioteca e criação mobile V2

## Estado

Codex, 2026-10-02. Branch `codex/nh-085-mobile-v2-biblioteca`, derivada de `mobile-shell`
em `c9b3a10`. PR #78 mesclado após CI 4/4 no head `4599a5719a5af50afac73a6b8f8d29cc2cbd0766`.
O usuário autorizou merge após CI aprovado. NH-085 permanece em andamento.

## Mudanças

Apresentação mobile de biblioteca e criação/edição, em claro e escuro. Cartão separa capa
compacta e conteúdo em fluxo; ações de 48px e formulário em folha. O popover da capa tem
overflow visível para não cortar Excluir. Desktop preserva a composição sobre a capa.
Avisos citam as funcionalidades existentes de sincronização, compartilhamento e backup.
Não foram alterados handlers, stores, router, portas nativas ou Sync V2.

## Validação e evidências

Build PASS; arquitetura 105 PASS; Rust 893 PASS/3 ignorados; shell Playwright 38 PASS/17
não aplicáveis. Inspeção geométrica: 16 combinações mobile, menus, confirmação de exclusão,
criar/editar/cancelar, espaço de teclado simulado e desktop nos dois temas em 1366/700px.
Detalhes e capturas: `docs/mobile/QUALIFICACAO_V2_FATIA_2.md`.

## Continuidade

Concluir CI da segunda fatia e integrar somente com todos os checks aprovados no head final.
Prosseguir com escrita/ferramentas. Não declarar Fase 5 concluída: demais páginas e M15 físico
pendentes. Sem novo APK, versão beta.13 mantida. Dispositivos mDNS/BLE/unificação adiados.
