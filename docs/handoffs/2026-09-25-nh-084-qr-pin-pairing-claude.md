# NH-084, PR B — pareamento por PIN assistido por QR — Handoff

```text
Agente:  Claude
Data:    2026-09-25
Branch:  nh-084-qr-pin-pairing (base: mobile-shell @ 7f31d1f, depois do PR #75)
Status:  REVIEW — falta o teste físico Windows ↔ S23
```

## Contrato aprovado (Opção A)

```text
QR → endpoint + PIN → SPAKE2 existente → Noise XXpsk0 → prova Ed25519 → SessaoAutenticada
```

Açúcar de UX sobre o PIN qualificado na Etapa I. **Nenhuma** mudança em wire, Hello, Noise, trust,
roster, protocolo, formato canônico ou schema. O QR criptográfico da ADR 0009 §6.1
(`infrastructure/sync_pairing.rs`) ficou intacto e não é reaproveitado — nota explícita na ADR 0009.

## O que foi feito

- **Rust (`application/sync_qr_pin.rs`)** — formato `narrahub-pair-pin:1:<ipv4>:<porta>:<8 dígitos>`,
  sem nome nem identidade. `montar` e `interpretar` estritos (prefixo, versão 1, IPv4 canônico com porta
  ≠ 0 e não especial, exatamente 8 dígitos, nada mais, até 64 caracteres). `parear_por_qr` interpreta e
  delega a `sync_sessao::parear_por_pin` — o mesmo caminho, sem atalho. Mensagens de erro não repetem a
  entrada; `Debug` esconde o PIN; o módulo não escreve em log.
- **Comandos** — `sync_v2_qr` (conteúdo do QR da escuta aberta; `None` sem código válido) e
  `sync_v2_parear_por_qr` (pareia pela string crua). Validade e uso único seguem os do PIN, na escuta.
- **Android** — `tauri-plugin-barcode-scanner` como dependência só de Android/iOS, inicializado em
  `cfg(mobile)`, e a capability `mobile-qr-scanner` com quatro permissões (ler, cancelar, consultar e
  pedir permissão de câmera).
- **Frontend** — porta `core/native/qr-scanner.service.ts` (devolve texto cru ou negado/cancelado/
  indisponível); `app-pairing-qr` desenha o conteúdo opaco com `uqr` (MIT, sem dependências); em
  Configurações → Sincronização, o QR aparece só com código válido e, onde há leitor, o botão
  "Escanear QR". Os campos de endereço e código continuam sempre — a leitura é atalho. Tela provisória
  até o PR E.

## Gates (cada um visto falhando)

| # | Gate | Onde | Mutação que o derruba |
| --- | --- | --- | --- |
| 1 / 9 | formato estrito; malformado recusado com o motivo certo | `sync_qr_pin::conteudo_malformado_e_recusado_com_o_motivo_certo` | QM1 qualquer versão, QM2 PIN de 7 |
| 2 | QR vencido falha pela validade **da escuta** | `qr_vencido_falha_pela_validade_da_escuta` | QM6 escuta sem validade |
| — | QR já usado não pareia de novo | `qr_ja_usado_nao_pareia_de_novo` | QM7 código não consumido |
| 3 | QR alterado (PIN errado) falha, ninguém entra | `qr_com_pin_alterado_falha_e_ninguem_entra` | — (mesmo mecanismo do PIN errado da Etapa I) |
| 4 | descoberta ≠ confiança | `descoberta_nao_e_confianca`, `qr_valido_pareia_e_o_roster_recebe_a_identidade_provada` | — |
| 9 | malformado não chega à rede | `qr_malformado_nao_chega_a_rede` | QM4 malformado conecta |
| 10 | PIN fora de log, mensagem e `Debug` | `pin_nao_aparece_em_mensagem_nem_em_debug`, `este_modulo_nao_escreve_em_log`, contrato da porta | QM3 `Debug` com PIN, QM5 log |
| 5 | só a porta nativa aciona o leitor; plugin só mobile; capability mínima | `rust-core-contract` | — |
| — | formato só no Rust (sem parser em TS) | `rust-core-contract` | QF4 parser em TS |
| 6 | QR só com código válido; escanear repassa a leitura crua | E2E `sync-session-feedback` | QF1 QR sem código válido |
| 8 | câmera negada → entrada manual utilizável | E2E | QF2 negação trava a tela |
| 11 | cancelar / leitor indisponível nunca remove a entrada manual | E2E | QF3 cancelar apaga o endereço |

## Validação

`cargo fmt --check`, `clippy -D warnings`, `sync_qr_pin` 11/11, build, `test:architecture` 104/104,
planning 4/4, ai 5/5, share-api 4/4, android-release 7/7, E2E `sync-session-feedback` 50/50 (5
viewports). Suíte Rust completa e CI: ver o PR.

## Falta

Teste físico: Windows mostra o QR → S23 escaneia → nenhum IP/PIN digitado → pareia → identidade
certa no roster → incremental funciona. Depois: QR vencido, QR já usado, QR alterado, câmera negada,
cancelamento. Exige uma beta Android com o plugin.
