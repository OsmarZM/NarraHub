# Roteiro físico — atualização Android N → N+1

> Prova que um aparelho com a versão N detecta a N+1 nas GitHub Releases, baixa o APK oficial,
> confere o SHA-256, abre o instalador do sistema, atualiza por cima e **mantém os dados**. O
> instalador do Android não é automatizável daqui; a parte automatizável está nos testes de
> `domain::versao`, `application::atualizacao_android` e `tests/android-release.test.mjs`.

Tempo: 20 a 30 minutos, depois de as duas releases existirem. Anote as evidências 📌.

---

## 0. Pré-requisitos

1. **Keystore de release e os quatro segredos configurados** — ver `docs/RELEASE_ANDROID.md`. Sem
   isso as releases não saem, e APK de debug não serve: ele não atualiza nem é atualizado.
2. **Duas releases publicadas pelo workflow**, com o mesmo keystore:

   | papel | versão | canal |
   | --- | --- | --- |
   | N | `0.10.0-beta.1` | pré-release |
   | N+1 | `0.10.0-beta.2` | pré-release |

   Cada uma com `NarraHub-Android.apk` e `NarraHub-Android.apk.sha256` nos assets.
3. Um aparelho Android **sem o NarraHub instalado**, ou com ele desinstalado. Um NarraHub de debug
   instalado impede a instalação da versão de release (assinaturas diferentes).

> Por que betas: pré-release não é oferecida a quem usa a estável, nem no Windows nem no Android.
> O teste acontece sem mudar nada para os usuários atuais.

---

## 1. Instalar a versão N e criar dados

1. No aparelho, abra a release **`app-v0.10.0-beta.1`** no GitHub e baixe `NarraHub-Android.apk`.
2. Instale. O Android pode pedir para liberar o navegador em "Instalar apps desconhecidos".
3. Abra o NarraHub. Configurações → Atualizações deve mostrar **Versão atual: 0.10.0-beta.1**.

📌 **E1** — foto de Configurações → Atualizações com a versão N.

4. Crie dados que cubram o que a atualização tem que preservar:
   - um universo chamado **"Teste de atualização"**;
   - um capítulo com uma frase e **uma imagem** inserida pelo editor;
   - em Configurações → Dispositivos, anote o **nome do aparelho**;
   - se possível, uma sessão de Sincronização nova para o aparelho ter identidade e roster.

📌 **E2** — foto do capítulo com a frase e a imagem.

---

## 2. Detectar a N+1

Com a release `app-v0.10.0-beta.2` publicada:

1. Feche o NarraHub por completo e abra de novo.
2. Em até alguns segundos deve aparecer o aviso **"NarraHub 0.10.0-beta.2 está disponível"**, com
   **Depois** e **Atualizar agora**.
   - Se não aparecer: Configurações → Atualizações → **Verificar atualizações**.

📌 **E3** — foto do aviso ou do cartão mostrando **Nova versão disponível 0.10.0-beta.2**.

**Contraprova rápida (opcional):** toque em **Depois**. O aviso some e o cartão volta a
**Verificar atualizações**. Verifique de novo para a novidade voltar.

---

## 3. Baixar e conferir

1. Toque em **Atualizar agora**.
2. Primeiro aparece o backup de segurança; depois **Baixando atualização… XX%**, avançando.
3. Termina em **Atualização pronta. Abrir instalador?**

📌 **E4** — foto de "Atualização pronta".

O SHA-256 foi conferido duas vezes antes dessa tela aparecer: no download e de novo antes de
liberar o botão. Um arquivo que não confere é apagado e a tela mostra erro — não chega a "pronta".

---

## 4. Instalador do sistema

1. Toque em **Abrir instalador**.
2. **Primeira vez:** o Android abre a tela "Instalar apps desconhecidos" para o NarraHub. Ative,
   volte ao NarraHub, e toque em **Abrir instalador** de novo.
3. O instalador do Android mostra **Atualizar** (não "Instalar" — "Atualizar" é o sinal de que a
   assinatura bate com a versão instalada).

📌 **E5** — foto do instalador do sistema com o botão **Atualizar**.

4. Confirme. Ao terminar, **Abrir**.

---

## 5. A versão N+1 com os dados da N

1. Configurações → Atualizações: **Versão atual: 0.10.0-beta.2**.
2. O universo **"Teste de atualização"** está lá.
3. O capítulo tem a **mesma frase** e a **imagem aparece**.
4. O **nome do aparelho** em Dispositivos é o mesmo.
5. Verificar atualizações agora diz que o NarraHub está atualizado.

📌 **E6** — foto de Configurações com a versão N+1.
📌 **E7** — foto do capítulo com a frase e a imagem, depois da atualização.

---

## Veredito

| evidência | prova | obrigatória |
| --- | --- | --- |
| E1 | versão N instalada | sim |
| E2 | dados criados na N, com imagem | sim |
| E3 | a N+1 foi detectada | sim |
| E4 | download concluído e conferido | sim |
| E5 | o instalador do sistema reconhece como atualização | sim |
| E6 | a N+1 está rodando | sim |
| E7 | os dados da N sobreviveram, imagem incluída | sim |

## Se algo falhar

| sintoma | onde olhar |
| --- | --- |
| a N+1 não aparece | a release é rascunho? tem os dois assets com o nome exato? a instalada é estável e a nova é beta? |
| "não confere com o SHA-256 publicado" | o `.sha256` da release corresponde a outro build; refaça a release |
| o instalador diz "Instalar" em vez de "Atualizar", ou recusa | as duas versões não foram assinadas com o mesmo keystore |
| "App não instalado" / conflito de pacote | havia um APK de debug instalado; desinstale e recomece do passo 1 |
| a imagem sumiu depois de atualizar | **defeito grave** — anote e não desinstale: os dados do app ainda estão no aparelho |
