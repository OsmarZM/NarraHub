# Sync V2, etapa I — qualificação física — Handoff

```text
Agente:  Claude
Data:    2026-09-24 a 2026-09-25
Branch:  sync-i-qualificacao-fisica (base: mobile-shell @ 3d29469, merge da H)
PR:      #74
Status:  REVIEW
```

## O que foi feito

Os gates I1–I20 rodaram em aparelhos reais — Windows 11 Home x64 e Samsung Galaxy S23 (SM-S911B,
Android 16) na mesma LAN —, com o app instalado pelos artefatos de distribuição: instalador NSIS do
perfil Qualification no Windows e o APK assinado das pré-releases no Android. O operador usou a tela do
celular; o lado Windows foi operado pelos **comandos do próprio app** (WebView2 com porta de depuração
local e `window.__TAURI_INTERNALS__.invoke`), e o celular pelo `adb` onde havia tempo crítico (cortar o
Wi-Fi, matar o app).

Resultado: **I1–I20 PASS**. O I6 usou uma terceira instalação desktop controlada (o transporte
Windows ↔ Android continuou físico); o I14 registra a limitação do segundo plano no Android; o I19 no
Android é por evidência indireta. Documento completo, com passos, SHAs, capturas e medidas:
`docs/qualification/SYNC_V2_PHYSICAL_QUALIFICATION.md`.

## Achados

| Id | O que era | Correção | Gate |
| --- | --- | --- | --- |
| I-BUG-01 | reinstalar no Android traz o acervo pelo backup do Google | nenhuma — identidade é nova (provado); documentado | — |
| I-BUG-02 | perfil Qualification abria sem a janela configurada (barra dupla, menus ocultos) | config repete a janela base | `migration-safety` |
| I-BUG-03 | fim de sessão invisível; o lado que escuta só mostrava dados ao reabrir | evento `sync-v2-sessao-atendida` + `SyncSessionFeedbackService` | contrato + E2E |
| I-BUG-04 | código vencido continuava na tela; "4 bytes" / "decrypt error" | escuta relê o estado; mensagens de código | Rust + E2E |
| I-BUG-05 | tela de conflitos sem rolagem, texto de 9 px, ids à mostra | rolagem própria, lado a lado / sequência, "e abrir para editar" | E2E |
| I-BUG-06 | Android não procurava a beta nova sozinho | arranque aceita o canal Android | `android-release` |
| I-BUG-07 | conflito entre decisões mostrado como certificado cru | detalhe mostra o capítulo que cada decisão produz | Rust |
| I-BUG-08 | mensagens de conexão culpavam a rede | silêncio / recusa / sumiço com instrução | Rust + E2E |
| I-BUG-09 | HTTPS do Rust no Android entrava em pânico; atualização pelo app nunca funcionou | TLS com raízes embutidas no atualizador | Rust + `android-release` |
| I-UX-04 | decisão definitiva sem aviso | confirmação em dois toques | E2E |
| I-ENV-01 | AppData virtualizado do agente (pacote MSIX) | processo lançado pelo `explorer` | — (ambiente) |

Nada no núcleo causal: protocolo 1, formato canônico 2, migration 29, wire, Hello, Noise e
`conflict_resolution` intactos.

## Pré-releases publicadas

`0.10.0-beta.3` … `0.10.0-beta.9` (só Android), cada uma depois de CI 4/4 no PR #74.

## O que ficou pendente

- ~~Prova física da atualização pelo app~~ — feita: o S23 na beta.8 ofereceu, baixou e instalou a
  beta.9 sozinho.
- Repetição física do I-BUG-07 (coberto por gate).

## Armadilha para o próximo agente

O Claude desktop é MSIX: processos lançados pelo agente gravam `%APPDATA%` numa cópia virtualizada.
Para testar o NarraHub instalado, lance-o com `explorer.exe <arquivo.cmd>` e copie bancos para `D:\`
também por um `.cmd` lançado assim (memória `claude-msix-appdata-virtualizado`).

## Próximo passo sugerido

NH-078 (QR e lista de aparelhos por perto), pedido do usuário durante a I; e decidir sobre um serviço em
primeiro plano no Android para a escuta.
