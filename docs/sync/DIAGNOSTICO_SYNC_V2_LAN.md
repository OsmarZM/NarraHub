# Diagnóstico — Sync V2 entre Windows e Android

> Levantado em 2026-09-15, na branch `sync-v2-lan` (a partir de `mobile-shell`, beta.2), seguindo
> o fluxo real de execução. **Nenhum código alterado ainda.**

## O fluxo de verdade

```text
SettingsPage (card "Sincronização nova (teste de campo)", o 3º de 4 cards de sync)
  → SettingsStore.pairSyncV2 / syncNowV2 / startSyncV2   (signals: syncV2State, syncV2Busy)
  → SyncV2Service.invoke('sync_v2_*')
  → interface/tauri/sync_v2_commands.rs                   (EstadoV2, thread "sync-v2-escuta")
  → application/sync_sessao.rs                            (modo em claro, papel, bootstrap, incremental)
  → application/sync_pin_pairing.rs                       (PIN → SPAKE2 → Noise XXpsk0 → prova Ed25519)
  → infrastructure/sync_wire.rs                           (quadro u32 + Noise, segmentação, tetos)
  → TcpStream / TcpListener
```

| item | como está |
| --- | --- |
| bind | `TcpListener::bind(("0.0.0.0", porta))` — correto para LAN |
| porta | 45870 fixa na tela (`SYNC_V2_DEFAULT_PORT`); V1 usa porta aleatória (`0.0.0.0:0`) |
| framing | `[u32 BE][ciphertext]`, segmentos de 65.518 bytes, teto de 64 MiB por mensagem |
| identificação do protocolo | **implícita**: o 1º quadro em claro é `narrahub.sync.v2.modo.pin` ou `.pareado` |
| versão de protocolo / app / schema | **não trocadas** |
| pareamento | PIN de 8 dígitos, 3 min, 3 tentativas → SPAKE2 → Noise XXpsk0 → prova Ed25519 |
| sessão pareada | Noise XX sem psk; autoriza pelo roster |
| connect | `connect_timeout` 10 s (`ESPERA_PADRAO`) |
| leitura/escrita | `SO_RCVTIMEO`/`SO_SNDTIMEO` de **60 s por operação** (`ESPERA_DA_SESSAO`), inclusive no handshake |
| escuta | uma thread, **uma conexão por vez**, com o mutex dos códigos preso a sessão inteira |
| erros | `DatabaseCommandError { kind, message }` — **sem código estável** |
| estado para a tela | `sync_v2_estado`: escutando, porta, endereços, PIN, último resultado, último erro (texto) |
| endereço mostrado | 1 IP, descoberto pela rota até `8.8.8.8` (socket UDP sem enviar pacote) |
| V1 | `src/sync.rs`, 4 comandos, **ainda com 2 cards na tela, antes do V2** |

## Causa raiz de "nada aconteceu"

São quatro, e se somam.

1. **Protocolos diferentes nas duas pontas.** Windows 0.9.2 só tem V1. Android beta tem os dois,
   e os cards do V1 ("Receber sincronização", "Conectar a outro dispositivo") vêm **antes** do V2,
   que se chama "teste de campo".
2. **V2 falando com algo que não é V2 espera até 60 s.** O visitante manda o modo e fica lendo o
   SPAKE2 com a espera de 60 s. Um V1 (ou qualquer outro programa) na porta não responde nada
   compreensível → um minuto de botão desabilitado, depois "não respondeu no tempo esperado".
   Se o anfitrião V2 recebe um modo desconhecido, **fecha sem responder**, e o visitante vê
   "a conexão caiu com N bytes por receber" — mensagem que aponta para a rede, não para versão.
3. **O erro aparece fora da vista.** A tela de Configurações mostra o erro num banner **no topo
   da página**. No celular os cards de sync estão bem abaixo: o erro existe, mas a pessoa não o vê.
   Durante a espera não há etapa nenhuma na tela — só o botão desabilitado.
4. **O anfitrião nunca conta o que houve.** Falhas do lado que escuta vão para `ultimoErro`, que a
   tela só lê quando o próprio usuário daquele aparelho clica em algo. Não há atualização.

## Problemas encontrados no caminho

| # | problema | efeito |
| --- | --- | --- |
| P1 | sem hello/versão/schema | incompatibilidade vira espera longa ou erro de rede enganoso |
| P2 | handshake com espera de 60 s | "nada aconteceu" por um minuto |
| P3 | escuta serial com mutex preso | uma conexão parada (V1, scanner, celular que saiu da rede) bloqueia a escuta por até 60 s |
| P4 | erros sem código | a tela não distingue recusa, timeout, firewall, PIN, versão, schema |
| P5 | endereço por rota até 8.8.8.8 | com VPN mostra o IP da VPN; sem rota para a internet, mostra **nenhum** |
| P6 | V1 visível e primeiro na tela | o usuário usa o protocolo errado |
| P7 | erro no topo da página | invisível no celular |
| P8 | sem estado de etapa | só `busy` booleano |
| P9 | release de pré-release é só Android | não existe Windows com V2 para testar |

## ⚠️ Problema arquitetural — dois acervos independentes

O V2 foi desenhado (ADR 0009 §14) para **um aparelho com acervo e outro vazio**: o vazio recebe o
snapshot como semente e, dali em diante, só eventos. `sync_sessao::depois_do_pin` decide o papel por
"fresco" de cada lado:

```text
eu fresco  ele fresco  papel     o que acontece
   sim        não      RECEPTOR  bootstrap: correto
   não        sim      DOADOR    bootstrap: correto
   sim        sim      PAR       nada a semear: correto
   NÃO        NÃO      PAR       admite o outro no roster e roda só o incremental   ← aqui
```

Na última linha, dois aparelhos que **nunca compartilharam acervo** — o PC vindo da 0.9.2 com
livros antigos e o celular com universos de teste — se admitem mutuamente e trocam apenas
**eventos**:

- tudo o que existia antes do V2 no PC não tem evento e **nunca viaja**;
- eventos de um lado citam agregados que o outro não tem (capítulo de um livro inexistente) e ficam
  pendentes ou viram conflito;
- a tela diz **"Sincronizado com PC do Osmar: 12 alterações recebidas"**, e os acervos continuam
  diferentes para sempre.

É exatamente o cenário do teste que você está tentando fazer. Não é defeito de transporte; é uma
regra que o protocolo não aplica. Corrigir o botão sem tratar isso produziria um "sucesso" falso.

**O que proponho (sem mudar causalidade, bootstrap nem roster):** recusar o pareamento por PIN
quando os dois lados têm acervo e o outro ainda não está no roster, **antes de admitir**, com erro
`INDEPENDENT_LIBRARIES` e a instrução: "o aparelho que vai receber precisa estar vazio". Re-pareamento
de um aparelho que já está no roster continua permitido. Juntar dois acervos independentes fica
fora do escopo, como o ADR já determina.

## Riscos para o teste com o seu PC

- **Instalar a beta no Windows substitui a 0.9.2** (mesmo identificador) e **migra o banco**. A 0.9.2
  não abre banco mais novo (tela de recuperação). Precisa de backup antes, ou de uma máquina/usuário
  de teste.
- **MSI não aceita versão com sufixo** (`0.10.0-beta.3`); por isso a pré-release hoje é só Android.
  Windows beta = **só NSIS** (`.exe`). O `latest.json` da beta não afeta a estável, porque o
  atualizador lê `releases/latest`, que ignora pré-release — mas a beta do Windows também não vai
  oferecer betas mais novas (mesmo motivo).
- **Android em segundo plano** congela o processo; a escuta no celular só funciona com o app aberto.
  Para o teste, o PC escuta e o celular conecta.
- **Firewall do Windows**: o primeiro `bind` em `0.0.0.0` dispara o aviso do Windows; recusado, ou
  com a rede como "Pública", o celular recebe timeout de conexão.
- **Android 16/17 — acesso à rede local**: há uma permissão nova de rede local em introdução no
  Android. Com target SDK 36 ela ainda não é exigida; precisa ser conferida antes de subir o target.
