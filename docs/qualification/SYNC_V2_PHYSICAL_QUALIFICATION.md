# Qualificação física do Sync V2 — Etapa I

Arquitetura do Sync V2 **congelada** a partir do merge da Etapa H (`3d2946911bc6730c098f5d5614177c01e045d063`,
PR #73). Esta etapa não redesenha protocolo, Hello, Noise, formato canônico, store-and-forward, bootstrap,
causalidade, grupos de mutação, `conflict_resolution`, tombstones nem recuperação legada. Defeito encontrado
aqui segue: reproduzir → gate automatizado quando possível → correção mínima → regressões → repetir o cenário
físico, e recebe um identificador `I-BUG-NN`.

Regra de registro: **PASS só com execução real** nos aparelhos listados. Sem hardware ou sem execução, o gate
fica `BLOCKED` — nunca PASS simulado, e emulador não conta como aparelho físico.

Branch: `sync-i-qualificacao-fisica` (base `mobile-shell` @ `3d29469`).

## I0 — Inventário do ambiente físico

### Windows

| Item | Valor | Fonte |
| --- | --- | --- |
| SO | Windows 11 Home Single Language 10.0.26200 (build 26200) | `Win32_OperatingSystem` |
| Arquitetura | x64 (AMD64) | `PROCESSOR_ARCHITECTURE` |
| Build do NarraHub em teste | 0.10.0-beta.3, perfil Qualification (`com.narrahub.app.qualification`), instalador NSIS local | ver I1 |
| Banco em teste | `%APPDATA%\com.narrahub.app.qualification\narrahub.db` | perfil Qualification |
| device_id | `KSJFZP2SJQEEOH4Q4PHFEMUY4GWMXAWP` | `sync_devices WHERE is_self = 1`, em cópia do banco |
| NarraHub de produção instalado | 0.9.2 em `D:\NarraHub`, acervo real em `%APPDATA%\com.narrahub.app` — **fora do teste** | registro de desinstalação |

Decisão do usuário (2026-09-24): o Windows qualifica pelo **perfil Qualification instalado por instalador
real**, não pela produção. O acervo real não participa. Linha de base do banco de produção, para provar que
nada o tocou:

```
narrahub.db  sha256 3ba40a99c41df1d6acac84a5f182d75487bfde802ff51a880891173369d96408
             6381568 bytes, modificado 2026-09-01 09:53:46 -0300
```

### Android

| Item | Valor |
| --- | --- |
| Fabricante/modelo | Samsung Galaxy S23 — `SM-S911B` (`dm1q`), serial adb `RQCX4004FTR` |
| Versão do Android | 16 (API 36), build `BP4A.251205.006.S911BXXSAFZH3` |
| ABI | `arm64-v8a` |
| Build do NarraHub | APK assinado da pré-release (`app-v0.10.0-beta.3`, depois `app-v0.10.0-beta.4`) |
| device_id | muda a cada reinstalação (ver I-BUG-01): `QFWEB3Z2…`, depois `YN4VBTJA…` |
| NarraHub antes da I | 0.10.0-beta.2 instalado desde 2026-09-15, desinstalado pelo usuário |

Leitura por `adb getprop`/`dumpsys`. O APK de release não é `debuggable`: o banco do Android **não** é
legível por `run-as`; a inspeção do lado Android é pela interface e pelo que o Windows recebe.

### Rede

| Item | Valor |
| --- | --- |
| Interface do Windows | Wi-Fi, `192.168.1.145` |
| Perfil de rede do Windows | **Público** |
| Firewall do Windows | ligado nos três perfis (Domínio, Privado, Público) |
| Regras existentes | entradas `NarraHub` / `narrahub_lib-*` liberadas no perfil Público (builds anteriores) |
| Mesma LAN que o Android | sim, depois de ajuste: o Android estava em outra rede (`FortatechMobile`, `192.168.2.47`) e foi movido para `FortatechCorp`, `192.168.1.127` — ping do Windows OK |

Observação: a regra de firewall é por executável. O instalador de qualificação é um executável novo; a
primeira escuta dele deve disparar o prompt do Windows — registrar no I2 se o pareamento dependeu de aceitá-lo.

## Matriz I1–I20

| Gate | Essencial | Status | Aparelhos | Direção | Evidência |
| --- | --- | --- | --- | --- | --- |
| I1 instalação real | sim | Windows PASS · Android pendente | Windows | — | §I1 |
| I2 fresh ↔ fresh | sim | **PASS** | Windows + S23 | PIN: Android → Windows | §I2 |
| I3 Windows doador → Android novo | sim | **PASS** | Windows + S23 | Windows doador | §I3 |
| I4 Android doador → Windows novo | sim | **PASS** | S23 + Windows | Android doador | §I4 |
| I5 bidirecional | sim | **PASS** | Windows + S23 | Android → Windows (pareado) | §I5 |
| I6 store-and-forward | não | pendente | | | |
| I7 conflito offline | sim | **PASS** | Windows + S23 | resolvido no Windows e no Android | §I7–I10 |
| I8 update × delete | sim | **PASS** | Windows + S23 | restaurar e manter exclusão | §I7–I10 |
| I9 decisão concorrente | sim | **PASS** | Windows + S23 | W no Windows × A no Android | §I7–I10 |
| I10 delete × delete | sim | **PASS** | Windows + S23 | exclusão nos dois | §I7–I10 |
| I11 blob real | sim | pendente | | | |
| I12 queda de rede | sim | pendente | | | |
| I13 kill / restart | sim | pendente | | | |
| I14 background / foreground | sim | pendente | | | |
| I15 pareamento negativo | sim | pendente | | | |
| I16 restart após bootstrap | sim | pendente | | | |
| I17 recovery legado | não | pendente | | | |
| I18 dataset maior | não | pendente | | | |
| I19 integridade final | sim | pendente | | | |
| I20 reinstalação / identidade | sim | pendente | | | |

## I1 — Instalação como usuário real

### Windows — PASS

- Artefato: `NarraHub Qualification_0.10.0-beta.3_x64-setup.exe` (NSIS), gerado por
  `tauri build --bundles nsis --config src-tauri/tauri.qualification.conf.json` a partir de `7f88328`.
  27 879 013 bytes, sha256 `a028f212fa0b2be8bd19efb15d8d22b8c239829f27676189ea79be7debd14bb1`.
- Dados antigos do perfil (agosto) renomeados para `*.pre-etapa-i-20260924`, não apagados.
- Instalação: `setup.exe /S /D=D:\NarraHubQualification`, saída 0. Registrado em "Aplicativos" como
  `NarraHub Qualification 0.10.0-beta.3`. Nenhum dev server nem console em execução.
- Primeira abertura: o banco aparece em 4 s, processo vivo. Fechamento pela janela: encerrou em menos de
  20 s, WAL consolidado (não sobra `-wal`/`-shm`).
- Cópia do banco: `integrity_check = ok`, `foreign_key_check` vazio, 29 migrations, 46 tabelas,
  `sync_devices` só com a linha `is_self = 1`, 0 eventos, 0 universos.
- Reabertura: processo vivo, fechamento limpo; mesmo device_id, 29 migrations, nenhuma reaplicada.
- Banco de produção (`com.narrahub.app`) com o mesmo sha256 da linha de base depois do ciclo.

### Android — pendente

APK: `NarraHub-Android.apk` da pré-release
[`app-v0.10.0-beta.3`](https://github.com/OsmarZM/NarraHub/releases/tag/app-v0.10.0-beta.3), assinado pela
workflow de release (a mesma distribuição do usuário), 44 601 618 bytes, sha256 conferido contra o
`.sha256` publicado. Aguardando o aparelho no `adb`.

## I2 — Fresh ↔ fresh — PASS

Build `0.10.0-beta.4` nos dois (Windows: instalador Qualification r3, sha256 `25fd12b4…`; Android: APK da
pré-release `app-v0.10.0-beta.4`, sha256 conferido). Os dois zerados: dados anteriores do perfil Windows
renomeados para `*.pre-i2-20260924`; no Android, `pm clear` depois da instalação (sem isso o backup do Google
traria o acervo antigo — I-BUG-01).

1. Pareamento por PIN, Android visitante → Windows escutando: sucesso, aviso de resultado nos dois lados.
2. Os dois apps fechados e reabertos; "Sincronizar pareado" só com o endereço, sem PIN: sucesso.

Cópia do banco do Windows depois: `integrity_check ok`, `foreign_key_check` vazio; roster com o próprio
`I7YQG4LR…` e o Android `OF3D7F4N…` (`active`, pareamento direto); 0 universos, 0 divergências. A segunda
sessão em modo pareado é relato do operador — o banco não registra o modo da sessão.

## I3 — Windows doador → Android novo — PASS

- **Acervo do Windows**: cópia do banco de produção do usuário (0.9.2; SHA do original conferido antes e
  depois — intocado) colocada no perfil Qualification. O arranque fez backup, migrou 0.9.2 → schema 29 (29/29)
  e adotou o acervo (51 eventos de gênese). Reforçado pelos **comandos do próprio app** (WebView2 com porta de
  depuração local e `window.__TAURI_INTERNALS__.invoke` — o mesmo caminho da interface, gerando eventos V2):
  texto com acentos, emoji, lista e citação em 2 capítulos; 2 capítulos novos; reordenação do livro; imagem
  nova num personagem; anexo; tag; cartão de planejamento; evento de timeline.
- **Doador**: 4 universos, 7 capítulos, 6 entidades (37 atributos), 4 relações, 5 eventos de timeline,
  2 itens de planejamento, 2 tags, 1 anexo, 5 blobs, 69 eventos. Impressão por capítulo (tamanho + SHA do
  texto) em `D:\DevTools\NarraHubTmp\etapa-i\i3\impressao-doador.txt`.
- **Android**: `pm clear` (identidade nova, receptor elegível).
- **Sessão**: PIN, Android visitante → Windows. Resultado no Windows: `papel = doador`,
  `houveBootstrap = true`; Android `MW2VPXOE…` admitido no roster só depois da semeadura.
- **Verificação no Android** (operador): universos, capítulos na ordem, textos, imagens, tags, planejamento e
  timeline presentes, e de novo depois de fechar e reabrir o app.
- **Igualdade causal**: "Sincronizar pareado" depois da reabertura → **0 alterações recebidas, 0 enviadas**.
- Limitação: o APK de release não é `debuggable`; o banco do Android não é lido diretamente. A igualdade é
  provada pelo vetor (0/0) e pela inspeção visual.
- Ocorrências: o celular voltou sozinho para a outra rede Wi-Fi (sessão sem resposta, nada gravado em nenhum
  lado); e o I-BUG-04.

## I4 — Android doador → Windows novo — PASS

Papéis e transporte invertidos em relação ao I3: o **Android escuta e doa**; o Windows, zerado (dados do I3
renomeados para `*.i3-20260924`), chega como visitante (comando `sync_v2_parear` do próprio app).

- **Acervo do Android**: o recebido no I3 mais conteúdo criado no celular pelo operador: texto editado em
  "o despertar", livro "O novo livro do teste" com um capítulo (sem texto, por escolha do operador), e
  personagem "O murim" com 14 atributos e **foto da galeria** do celular.
- **Sessão**: 7,1 s. Windows: `papel = receptor`, `houveBootstrap = true`, `blobsRecebidos = 6`.
- **Banco do Windows** depois: `integrity_check ok`, `foreign_key_check` vazio, 0 divergências, 0 pendentes.
  Comparado com a impressão do doador do I3: tudo igual, exceto exatamente o que o operador mudou no
  celular ("o despertar" 361 → 389 caracteres; livro, capítulo e personagem novos).
- **Blobs**: 6 arquivos, cada um com nome igual ao SHA-256 do conteúdo (inclusive a foto de 223 KB e uma
  imagem de 1,5 MB).
- **Igualdade causal**: "Sincronizar pareado" do Windows logo depois → `par`, 0 recebidas, 0 enviadas.
- Observação: o receptor semeado não tem linhas em `sync_events` (o bundle traz estado, roster e linha de
  base dos cursores, não o log) — é o contrato do bootstrap da etapa 12, não perda.

## I5 — Bidirecional real — PASS

Sem sincronizar no meio:
- **Windows** (comandos do app): criou "Capítulo W (Windows)" com texto; editou a descrição de "Reino magico".
- **Android** (operador): criou "Capítulo a android" com o texto "Teste de sync" no livro "teste de livro".

Sessão pareada iniciada no Android contra o Windows escutando. Resultado no Windows: `par`, **3 eventos
aplicados** (criação do capítulo, posição, texto), 0 pendentes, 0 conflitos. O operador confirmou "Capítulo W"
e a edição de "Reino magico" no celular, e o aviso de fim de sessão apareceu sozinho na janela do Windows
(repetição física do I-BUG-03). Segunda sessão: **0/0** (idempotente), repetida várias vezes.

Nesta rodada a edição de personagem foi só do lado do Windows; edição de personagem originada no Android já
tinha sido provada no I4 ("O murim", com foto).

**Incidente de ambiente, não do produto (ver I-ENV-01):** a primeira tentativa do I5 foi perdida porque o
processo do NarraHub que eu tinha aberto morreu com a minha sessão e, ao reabrir por outro caminho, o app
passou a ler a pasta de dados real e não a virtualizada. O estado do I5 foi levado para a pasta real e o teste
refeito do zero.

## I7–I10 — Conflitos, decisões e exclusões — PASS

Build `0.10.0-beta.5` nos dois. "Offline" aqui é **sem sessão**: a sincronização só acontece quando alguém a
inicia, e a desconexão por modo avião já tinha sido exercitada pelo operador. Lado Windows feito pelos
comandos do app; lado Android pelo operador na interface.

**Rodada 1 — conflito W × A em "Um novo capítulo"** (criado pelo operador; textos "Nesse teste…" × "Nesse
avião…"). Os dois aparelhos mostraram o conflito com as duas versões. Esta rodada revelou **I-BUG-05** (tela
sem rolagem, 9 px, campos técnicos); com a beta.5 a tela foi repetida e aprovada pelo operador nos dois. O
operador resolveu nos dois aparelhos escolhendo **a mesma versão** (a do Windows): as duas decisões produzem a
mesma revisão (determinística), chegam como já presentes, e **nenhum conflito novo** surge — convergência de
decisões iguais.

**Rodada 2 — cinco capítulos numa só sessão** (sem sincronizar entre as alterações):

| Capítulo | Windows | Android | Sessão | Decisão | Final (Windows = Android) | Gate |
| --- | --- | --- | --- | --- | --- | --- |
| outro capiitulo | edita "VERSÃO W de X" | edita "VERSÃO A de X" | conflito | **só no Android** | versão escolhida no Android chega ao Windows | I7 |
| teste de capitulo | edita "VERSÃO W de Y" | edita "VERSÃO A de Y" | conflito | Windows fica com W, Android fica com A | **conflito novo sobre a decisão** nos dois | I9 |
| a primeira magia | edita | exclui | conflito | Windows: **restaurar** | restaurado com o texto W nos dois | I8 |
| Epílogo provisório | edita | exclui | conflito | Windows: **manter exclusão** | excluído nos dois | I8 |
| Capítulo a android | exclui | exclui | **sem conflito** | — | excluído nos dois, nunca volta | I10 |

- A sessão produziu exatamente **4 conflitos nos dois aparelhos**; o delete × delete convergiu sozinho.
- Depois das decisões e da sessão: o Android ficou com 1 conflito — o novo, "Os dois aparelhos resolveram o
  mesmo conflito de formas diferentes" — e os dois do Windows chegaram resolvidos. O operador estranhou a conta
  (2 → 1); é a conta certa.
- O conflito sobre a decisão foi resolvido no Android; sessão seguinte e a próxima: **0 conflitos, 0/0**. No
  Windows: 0 abertos, 0 pendentes, "teste de capitulo" = "VERSÃO A de Y".
- Achados desta rodada: **I-BUG-07** (conflito sobre decisão mostrado como certificado cru) e **I-UX-04** (a
  decisão é definitiva e a tela não avisava — o operador escolheu "… e abrir para editar", quis voltar atrás
  e o conflito já não existia).

## Bugs encontrados

### I-BUG-01 — backup automático do Android restaura o acervo numa instalação nova — **não é defeito de identidade**

- **Observado** (Samsung SM-S911B, Android 16, beta.3): depois de desinstalar a beta.2, a instalação nova
  (`firstInstallTime` 10:35:05) abriu com o universo antigo "Teste murim". O `dumpsys backup` registra
  restauração do pacote 4 s depois da instalação. O manifest não declara `allowBackup`: vale o padrão do
  Android (ligado), e o Google restaura os dados do app na reinstalação.
- **Hipótese investigada**: a chave privada (`sync-identity.json`) voltar junto, e a instalação nova assinar
  como a antiga — o que o ADR 0009 §5 proíbe.
- **Reprodução física** (roteiro de I20): pareado com o Windows, o Android apresentou
  `QFWEB3Z2M3URVCKCQQBWC4SGPW3NDNBZ`; desinstalado e reinstalado o mesmo APK (nova restauração registrada
  às 11:14:09), o pareamento seguinte apresentou **`YN4VBTJA2CZK55ZNVPLWZCMVP4XWFRUU`**. Identidade nova.
- **Conclusão**: a identidade **não** é clonada. O banco restaurado é reconciliado no arranque (o `self`
  antigo é rebaixado e a gênese da identidade nova readota o acervo), exatamente o caso previsto no
  `identity_store.rs`. Sem correção. Fica registrado como comportamento: no Android, reinstalar **pode
  trazer o acervo de volta pelo backup do Google** (não apaga); o aparelho reinstalado é outro aparelho
  para a sincronização e precisa ser pareado de novo. O aparelho antigo continua no roster dos outros como
  `active`.
- Evidência: `D:\DevTools\NarraHubTmp\etapa-i\i-bug-01\` (dumpsys, capturas, cópias do banco do Windows
  após cada pareamento).

### I-BUG-02 — perfil Qualification abria sem a janela configurada — **corrigido** (`aa1d3f3`)

- **Observado** (Windows 11): barra do Windows por cima da barra do app (dois conjuntos de
  fechar/maximizar/minimizar) e, com a janela estreita, tema e configurações ocultos.
- **Causa**: o merge de configuração do Tauri substitui arrays. `tauri.qualification.conf.json` declarava
  `app.windows` só com o título e apagava `decorations: false`, `maximized` e os tamanhos; abaixo de 900 px o
  layout esconde as ações do topo. A produção não sobrescreve `app.windows` e não tinha o defeito.
- **Correção mínima**: o perfil repete a entrada base inteira, mudando só o título.
- **Gate**: `tests/migration-safety.test.mjs` — perfil que sobrescreve a janela tem de repetir a base; falha
  com o arquivo antigo.
- **Repetição física**: instalador r2 — janela maximizada, uma barra só, menus visíveis.

### I-BUG-03 — o fim de uma sessão de sync não aparecia na tela — **corrigido** (`865df51`)

- **Observado** (Windows ↔ Android, pareamento por PIN): o celular mandou "Teste murim" para o Windows e o
  Windows mandou "teste" para o celular — o banco dos dois estava certo —, mas o aparelho que **escutava**
  só mostrou o que chegou depois de reabrir o app, e nenhum dos dois disse que algo tinha acontecido.
  Confirmado reabrindo o app do celular: "teste" estava lá.
- **Causa**: a sessão atendida pela escuta roda numa thread do Rust e grava no banco sem que o frontend
  saiba; só quem inicia a sessão recebe o resultado do comando, e ainda assim a biblioteca não era relida.
- **Correção mínima**: a escuta emite `sync-v2-sessao-atendida` ao fim de cada sessão;
  `SyncSessionFeedbackService`, ouvindo desde o arranque, mostra o resumo (recebidas/enviadas/imagens) e
  relê biblioteca, universo ativo e conflitos — o mesmo caminho para quem iniciou. Protocolo, wire e core
  causal intocados.
- **Gates**: `tests/rust-core-contract.test.mjs` (Rust emite, frontend ouve o mesmo nome, arranque liga o
  ouvinte) e `tests/e2e/sync-session-feedback.spec.mjs` (5 viewports: aviso + releitura na sessão atendida,
  erro sem fingir sucesso, e o lado que inicia). Os dois falham sem a correção.
- **Repetição física**: pendente com a beta.4.

### I-BUG-04 — código de pareamento vencido continua na tela, e o erro no visitante é ilegível — **corrigido**

- **Observado**: o Windows mostrava o código `4288 3074`; o celular pareou com ele e recebeu "a conexão caiu
  com 4 bytes ainda por receber". Estado da escuta do Windows no mesmo instante: `pin: null`, porque o código
  tinha vencido (3 minutos).
- **Causa**: a tela da escuta só relê o estado quando o usuário age, então o código vencido continua
  visível; e o anfitrião sem código aberto fecha a conexão (`unico_aberto()` falha antes de responder), e o
  visitante só vê o fim do fluxo.
- **Correção mínima** (sem mudar protocolo nem mensagens do fio):
  - a tela da escuta relê o estado a cada 5 s enquanto escuta e, sem código aberto, diz "O código venceu ou
    já foi usado. Toque em Novo código…"; com código, diz que ele vale três minutos e três tentativas;
  - o visitante traduz a conexão encerrada antes da primeira resposta do modo PIN em "O outro aparelho não
    aceitou o código…", e o canal que não fecha com a chave do código em "O código não confere…";
  - o anfitrião registra "Um aparelho tentou parear com um código que não confere" (antes: "handshake:
    decrypt error").
- **Gates**: `sync_pin_pairing::codigo_vencido_diz_que_o_codigo_nao_foi_aceito` (novo; estável em 8
  repetições), `pin_errado_nao_pareia_e_ninguem_entra_no_roster` agora confere as duas mensagens, e
  `tests/e2e/sync-session-feedback.spec.mjs` "código vencido aparece como vencido…" (falha sem a correção).
- Contorno antes da correção: "Novo código" e digitar logo em seguida.
- **Repetição física**: pendente com a beta.5.

### I-BUG-05 — tela de conflitos não rolava, texto em 9 px, campos técnicos à mostra — **corrigido**

- **Observado** (Windows e S23, conflito real de capítulo): a página ficava parada — o segundo botão de
  decisão cortado embaixo, alcançável só com janela grande —, os valores das versões em letra minúscula, o
  `bookId` e os enums internos (`CANON`, `IDEIA`) à mostra e a tabela "Campo a campo" vazia. O operador pediu
  os capítulos lado a lado no Windows, as linhas em conflito em sequência no celular, e poder escrever um
  texto que não é nenhum dos dois.
- **Causas** (medidas no app real via WebView2): `.content-stage` tem altura fixa e `overflow: hidden`, e a
  página de conflitos (1001 px numa janela de 736) não tinha rolagem própria; a regra global
  `.workspace-card small, .version { font-size: 9px }` pegava as caixas `.version` da tela.
- **Correção** (só frontend da tela de conflitos): a página rola sozinha, como Configurações; classes com
  nome próprio; texto do capítulo parágrafo a parágrafo, **lado a lado** a partir de 700 px, com o que difere
  destacado; na tela estreita, **só as linhas que mudaram, uma seguida da outra**, com as iguais resumidas;
  identificadores escondidos e campos iguais recolhidos; tabela só quando há diferença fora do texto;
  "Copiar texto" em cada versão; e **"… e abrir para editar"**: resolve com a versão escolhida e abre o
  capítulo no editor. A resolução continua sendo escolher uma versão — o formato congelado não carrega texto
  novo —, e o ajuste depois é uma edição comum, que sincroniza como qualquer outra.
- **Gates**: dois testes novos em `tests/e2e/sync-conflicts.spec.mjs` (a página rola com a roda, fonte
  ≥ 13 px, sem identificador, parágrafo divergente destacado; "e abrir para editar" resolve e navega), os
  dois vermelhos com a tela antiga; os três testes anteriores continuam verdes nos 5 viewports.
- **Repetição física**: pendente com a beta.5.

### I-ENV-01 — o ambiente do operador virtualizava o AppData do Windows — **não é defeito do produto**

- **Observado**: o app reaberto às 16:42 mostrou identidade nova (`3RQTOFJE…`) e 0 universos, e o celular
  pareado foi recusado ("handshake: decrypt error"). Parecia perda do acervo.
- **Causa**: o agente que conduz a I roda dentro do app Claude, que é um pacote MSIX. Todo processo iniciado
  por ele — inclusive o NarraHub aberto com `Start-Process` do I1 ao I5 — grava `%APPDATA%` numa cópia
  virtualizada (`%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming`). O NarraHub aberto fora do
  pacote viu a pasta real, sem acervo, e fez o que uma instalação nova deve fazer. A recusa do celular diante
  de um "Windows" desconhecido foi o comportamento de segurança correto.
- **O que isso muda na evidência**: I1–I4 usaram a cópia virtualizada como pasta de dados; o app, o
  instalador, o transporte e a rede eram reais. A partir do I5 o NarraHub roda **fora do pacote** (lançado
  pelo `explorer.exe` com um `.cmd`), com a pasta real — o ambiente de um usuário — e as leituras do banco são
  cópias feitas também fora do pacote. O banco de produção nunca foi escrito em nenhuma das duas visões.

### I-BUG-06 — o Android não oferecia a beta nova sozinho — **corrigido**

- **Observado**: com a beta.5 publicada, o celular na beta.4 não ofereceu a atualização, nem reaberto. A API
  do GitHub devolvia a beta.5 corretamente (pré-release, com APK e SHA-256).
- **Causa**: o arranque só chamava a verificação quando `updater_configured` — o atualizador do **desktop** —
  estava configurado, o que nunca é verdade no Android. O canal do Android só rodava pela tela Configurações.
- **Correção**: `shouldCheckForUpdatesOnStartup()` aceita o canal do Android. Gate em
  `tests/android-release.test.mjs` (vermelho sem a correção).
- **Repetição física**: exige duas versões seguidas — instalar a corrigida e ver o app oferecer a próxima.
  A beta.5 foi instalada por `adb install -r` (por cima, dados preservados).

### I-BUG-07 — conflito entre decisões mostrado como certificado cru — **corrigido**

- **Observado** (S23): "Sem título", "Escolha a" e a lista `results` com `aggregateId`, `baseRev`,
  `otherRev`, `resultRev`.
- **Correção** (só apresentação, `application/conflitos.rs`): para `conflict_resolution`, cada lado mostra o
  item que aquela decisão produz — o capítulo na revisão resultante, com texto e diff — e o título é o do
  capítulo. Certificado, formato e protocolo intocados.
- **Gate**: `resolucao_testes::ibug07_conflito_entre_decisoes_mostra_o_capitulo_que_cada_uma_produz`
  (vermelho sem a correção).
- **Repetição física**: pendente com a beta.6.

### I-UX-04 — a decisão de conflito é definitiva e a tela não avisava — **corrigido**

- O primeiro toque num botão de decisão só arma e avisa ("encerra o conflito nos dois aparelhos e não pode ser
  desfeita"); o segundo decide. Gate no E2E de conflitos. Para mudar de ideia depois de decidir: editar o
  capítulo normalmente.

### Observações de UX (sem correção nesta etapa)

- **I-UX-01**: o código de pareamento só aparece depois de "Escutar nesta rede" — o usuário não o
  encontrou sem orientação.
- **I-UX-02**: o nome padrão do aparelho no Android é "Meu computador".
- **I-UX-03**: no desktop, com a janela entre 760 e 900 px (o mínimo permitido é 760), as ações do topo somem.
