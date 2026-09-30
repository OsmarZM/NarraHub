# NH-085 — Auditoria e propostas de design mobile V1

```text
Agente: Codex
Data: 2026-09-30
Branch: codex/nh-085-auditoria-design-mobile
Base: mobile-shell @ 3e91b843c90093bcfea0a966a39ca839601a56ca
Status: M0 parcial; 14 propostas visuais entregues; implementação e qualificação pendentes
```

## Decisões e autorização

O usuário adiou mDNS, BLE e tela unificada de dispositivos e autorizou avançar para a Fase 5.
Forneceu 14 prints do S23 e pediu uma imagem de design por página como referência para ajustes.
Após a primeira exploração, corrigiu explicitamente a direção: **manter o menu de relógio
lateral; não usar menus inferiores**. Essa preferência prevalece sobre o antigo item M2 do roadmap.

## Entrega

- `docs/mobile/UX_AUDIT_V1.md`: achados visuais por print, classificação, prioridades e pendências.
- `docs/mobile/audit-2026-09-30/screenshots/`: originais extraídos do ZIP, sem alteração.
- `docs/mobile/design-v1/`: 14 PNG finais, uma proposta por página/estado enviado.
- `docs/mobile/design-v1/index.html`: seleção de página, comparação original/proposta e ampliação.
- `PROMPTS.md` e `REVISOES.md`: padrão comum, prompts por página, ajustes finais e origem dos arquivos.
- TASKS, PROJECT_STATE e ROADMAP: Fase 5 ativa, 4.5 parcialmente concluída com escopo adiado;
  release 1.0 ainda exige resolver explicitamente o escopo adiado.

Ferramenta de imagem integrada; nenhuma dependência nova no produto. A exploração com menu
inferior foi descartada. Em 03/11 houve edição pontual de cópia para evitar promessa incorreta de
dados exclusivos no aparelho quando existe sync/API própria. Só as revisões selecionadas estão
na galeria final. Capas e narrativas são exemplos, não alteração do acervo.

## Evidência de plataforma

`src-tauri/src/local_ai.rs`, comando `local_ai_status`: suporte de instalação gerenciada é
`cfg!(all(windows, target_arch = "x86_64"))`. A configuração atual do Android exibe recomendações
de computador apesar desse contrato. Proposta 11 mostra a limitação; não instala IA nem cria
Context Engine. Sincronização da proposta 12 reorganiza somente escuta/PIN/QR existentes.

## Validação executada

- Inspeção visual dos 14 prints e das 14 propostas finais retornadas pelo gerador.
- `tests/docs-consistency.test.mjs`: 7/7 PASS.
- `release:validate-version`: beta.13 consistente; nenhum manifesto alterado.
- `git diff --check`: PASS.
- Galeria: 14 PNG finais, 14 prints, 28 referências de imagem com arquivos existentes.
- Preview local da galeria: HTTP 200 em `http://127.0.0.1:8766/design-v1/index.html`.

## Próximo trabalho

Implementar composição por fatias conforme ADR 0011, preservando relógio lateral e handlers
compartilhados. Prioridades: sobreposição de categorias/criação, toolbar e configuração; depois
editor/teclado, biblioteca/formulários, planejamento/timeline/relações e refinamentos.

M0 continua parcial: faltam estados preenchidos, formulários de entidades/cards/marcos,
teclado, tema claro, paisagem, erros e outros detalhes de rotas. M15 não executado. Nenhum
componente do aplicativo foi modificado, nenhuma beta foi gerada e nenhum novo fluxo foi
qualificado por este trabalho. Commits de design são locais; não houve publicação remota
dos prints ou das propostas.
