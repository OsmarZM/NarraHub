# NH-084, PR B — pareamento por PIN assistido por QR — Handoff

```text
Agente:  Claude
Data:    2026-09-25
Branch:  nh-084-qr-pin-pairing (base: mobile-shell @ 7f31d1f, depois do PR #75)
Status:  DONE — beta.13 qualificada no S23; PR #76 mesclado em 2026-09-30
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

## Teste físico da beta.10 → ajustes da beta.11

O S23 leu o QR do Windows e pareou (o QR, já usado, não pareia de novo — correto). Três problemas de tela:

1. **Tudo cinza depois do QR** — o endereço vinha no QR mas não ia para o campo, e "Sincronizar pareado"
   exige endereço. Agora `sync_v2_parear_por_qr` devolve `{ resultado, endereco }` (o endereço que o Rust
   validou; nunca o PIN), a tela preenche o campo e lembra o último endereço (`narrahub.syncV2.lastAddress`)
   depois de parear ou sincronizar, por QR ou à mão.
2. **Sem aviso durante a sessão** — os botões apagavam sem explicação. Agora há a linha
   "Conectando e sincronizando com o outro aparelho…" (`role=status`) enquanto a sessão corre.
3. **QR sempre aberto, mal posicionado** — agora só o botão "Mostrar QR"; abre uma janela (folha de baixo no
   celular, centralizada no Windows) com QR de 260 px, endereço e código; fecha sozinha quando o código é usado
   ou vence, e um código novo não a reabre.

| Gate | Mutação vista falhando |
| --- | --- |
| QR só abre quando pedido e fecha quando o código some | QF5 abrir sozinho com código, QF8 não fechar |
| endereço do QR preenchido e "Sincronizar pareado" habilitado | QF6 esquecer o endereço |
| aviso visível durante a sessão | QF7 sem aviso |

Geometria medida nos 5 viewports: folha ocupa a largura toda no celular, sem rolagem horizontal; no desktop
1366×768 fica centralizada com 460 px. E2E `sync-session-feedback` 60/60, arquitetura 104/104,
`sync_qr_pin` 11/11, fmt e clippy limpos.

## Teste físico da beta.11 → ajustes da beta.12

Passaram no aparelho: caminho feliz (QR → pareia sem digitar → endereço preenchido → sincronização), QR
antigo não volta sozinho, QR já usado recusado, QR alterado recusado ("código que não confere. Nada foi
gravado"), QR vencido some da tela e pede código novo. Dois bugs:

1. **Não havia como sair da leitura** — o plugin em tela cheia (`windowed: false`) põe a câmera por cima
   da WebView sem botão, e o "voltar" do Android não cancelava. Agora `windowed: true`: a câmera fica por
   baixo, a página fica transparente (`html.nh-lendo-qr`) e mostra só mira, instrução e **Cancelar**; o
   "voltar" do Android cancela (`onBackButtonPress` só enquanto lê).
2. **Negar a câmera travava** — depois de negar, o Android não pergunta de novo. Agora a tela oferece
   **Abrir permissões do app** (`openAppSettings`; capability ganhou `allow-open-app-settings`).

| Gate | Mutação vista falhando |
| --- | --- |
| mira + Cancelar; cancelar devolve a tela inteira e a página não fica transparente | QF9 Cancelar sem efeito, QF10 transparência não desfeita |
| câmera negada oferece as permissões do app | QF11 sem o botão |
| leitura por baixo da tela; "voltar" cancela (contrato da porta) | QC1 tela cheia de novo, QC2 voltar sem cancelar |

Fundo medido transparente nos 5 viewports durante a leitura (a transição de fundo foi desligada; antes a
câmera apareceria atrás de um véu de 78%).

## Diagnóstico físico da beta.12 (2026-09-28)

A pré-release Android `app-v0.10.0-beta.12` foi publicada no commit
`744b74f900d6696cf697f55f70c3e7ac0f0f2c7f`. No diagnóstico, o PR #76 estava aberto com esse
head, base `mobile-shell` e CI 4/4 verde. Isso não qualifica o comportamento físico.

- **T1, Cancelar: FAIL.** No S23, a câmera abre e fecha após tocar Cancelar; o processo continua vivo,
  mas a camada da leitura permanece e a tela fica presa. O Voltar do Android ainda precisa ser repetido.
  No `tauri-plugin-barcode-scanner` 2.4.6, `cancel()` chama `destroy()` antes de rejeitar a leitura;
  `destroy()` limpa `savedInvoke`, portanto a promessa de `scan()` fica pendente e a tela não sai do
  `await`. Essa é a causa identificada para o Cancelar.
- **T2, permissão negada: não qualificado.** Foi relatada tela presa e ausência de novo prompt; esse
  cenário ainda não foi isolado fisicamente. Não atribuir a ele a causa de T1 sem reprodução.

Na data desse diagnóstico, a beta.13 ainda não havia sido publicada. Os cenários de pareamento,
roster, sincronização, QR vencido/usado/alterado já tinham passado fisicamente na beta.11, como
registrado acima.

## Correção preparada na beta.13 (2026-09-29)

O plugin 2.4.6 está fixado como crate local em `src-tauri/vendor/tauri-plugin-barcode-scanner`.
Em `cancel()`, a referência de `savedInvoke` é preservada antes de `destroy()` e rejeitada em
seguida. A tela sai da leitura imediatamente ao tocar Cancelar, sem esperar a Promise nativa;
uma leitura tardia é ignorada. O Voltar do Android usa o mesmo caminho de cancelamento. O serviço
também verifica cancelamento após carregamento do plugin, permissões e registro do Voltar, para
não abrir a câmera depois que a página já saiu.

Validação local: preflight verde (build, 105 testes de arquitetura, 5 de IA, 4 de planning,
4 de share API, validações de release e 893 testes Rust com 3 ignorados); 7 testes de release
Android passaram; `cargo fmt --check` passou; compilação Kotlin do plugin Android passou.
Os dois novos E2E do cancelamento passaram e a mutação negativa que remove a saída imediata
fez o teste falhar. A suíte móvel completa teve 3 falhas no teste preexistente de peteleco
curto da navegação em diferentes viewports; na repetição isolada, 3 viewports passaram e 1
falhou. Esse teste não cobre QR e não foi alterado nesta correção.

## Qualificação física da beta.13 (2026-09-29)

A pré-release `app-v0.10.0-beta.13` foi publicada como APK Android assinado com SHA-256. A tag
aponta para `b689dbe069752c322334438818ff450360005480`; o PR #76 estava aberto, com base
`mobile-shell`, mergeable e CI 4/4 verde nesse head. No Galaxy S23, o usuário confirmou:

- **T1 PASS:** a câmera abre nítida com mira; Cancelar e Voltar do Android fecham o scanner e
  devolvem o app utilizável. Endereço e PIN manuais permanecem editáveis, sem criar sessão.
- **T2 PASS:** com câmera negada, aparece o aviso e o botão Abrir permissões do app. Após liberar
  a câmera nas configurações e voltar, o scanner abre nítido, com mira, sem véu cinza nem tela
  presa; Cancelar funciona.

**PR B physical qualification = PASS; QR-assisted pairing = QUALIFIED.** Nenhum código mudou
depois da beta.13. Este registro é documental e, se formar um novo head do PR, o SHA da beta.13
permanece seu ancestral de código. Não fazer outra beta por essa atualização documental. PR #76
ficou **READY FOR MERGE** após essa confirmação física.

## Fechamento do PR B (2026-09-30)

O usuário autorizou explicitamente o merge quando os checks passassem. Angular, Mobile
(Playwright), Core Rust e Android estavam concluídos com sucesso no head
`2b64623265d8b9e67ab38c5c486addbccde210c6`. O PR #76 foi mesclado em `mobile-shell`,
com conferência desse head no comando de merge, às 11:50:06 UTC de 2026-09-30.

- Merge: `2e9c3810ea8527b52d7f226e1a9346e983fa83df`.
- Estado remoto confirmado: `MERGED`; base `mobile-shell`.
- Base local atualizada por fast-forward para o merge.
- Beta.13 continua sendo o APK qualificado; o fechamento não muda código nem gera outra beta.
- PR B: **DONE**. NH-084 continua **IN_PROGRESS**.
- Próxima fatia: PR C (mDNS). O plano apresentado aguarda revisão antes de implementar código,
  conforme o contrato de uma fatia por PR e plano revisado antes do código.
