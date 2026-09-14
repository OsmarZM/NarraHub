# Etapa 14 — Roteiro do teste físico Windows ↔ Android

> O gate final de plataforma. Executa **exatamente o mesmo cenário** do gate automatizado
> `application::sync_sessao::tests::windows_e_android_convergem_ponta_a_ponta_sobre_tcp_real`,
> só que em dois aparelhos reais na mesma rede. O automatizado prova o protocolo; este prova
> que Windows, Android, Wi-Fi, firewall e WebView não quebram nada no caminho.

Tempo esperado: 20 a 30 minutos. Anote cada **evidência** marcada com 📌 — é ela que fecha o
gate, não a impressão de que funcionou.

---

## 0. Preparação

**O APK sai do pipeline, não da máquina de ninguém.** Na PR da etapa 14 (#54), job `Android`,
artefato `narrahub-debug-apk`. É um APK de debug: instala por cima de nada, com dados próprios.

| aparelho | o que precisa |
| --- | --- |
| **Windows (A)** | o NarraHub da branch `etapa-14-e2e-windows-android` (`npm run desktop:dev`) |
| **Android (B)** | o APK do artefato, **instalação nova** — sem acervo |
| **rede** | os dois no **mesmo Wi-Fi privado**. Rede de visitante e hotspot corporativo costumam isolar clientes, e aí nada conecta — isso é a rede, não o app |
| **cabo** | opcional: `adb` só é necessário para a evidência do SHA no Android |

**Firewall do Windows:** na primeira vez que a escuta abrir, o Windows pergunta se permite o
NarraHub na rede. Permita em **rede privada**. Se a pergunta não aparecer e o Android não
conectar, libere a porta TCP **45870** de entrada.

**Não use a sincronização antiga em nenhum momento deste roteiro.** A tela trava um enquanto o
outro está ligado; se algum botão antigo aparecer habilitado com a escuta nova ligada, isso é
defeito — anote.

---

## 1. Acervo no Windows (A)

1. Crie um universo, uma história, um livro e um capítulo chamado **"Capítulo do teste"**.
2. No capítulo, escreva uma frase e **insira uma imagem pelo botão de imagem do editor**.
   Qualquer PNG ou JPG com menos de 8 MB.
3. Salve.

📌 **E1** — foto da tela com o capítulo e a imagem visíveis no Windows.

---

## 2. Pareamento por PIN e bootstrap (A → B)

**No Windows (A):** Configurações → Dispositivos → cartão **"Sincronização nova (teste de
campo)"** → **Escutar nesta rede**.

A tela mostra um **endereço** (algo como `192.168.0.10:45870`) e um **código de oito dígitos**.
O código vale **três minutos** e **três tentativas**.

Se nenhum endereço aparecer, a rede não tem rota padrão e o app não conseguiu descobrir o IP
sozinho. Pegue o IPv4 do Wi-Fi em `ipconfig` e use `IP:45870`.

**No Android (B):** abra o NarraHub pela primeira vez → Configurações → Dispositivos → mesmo
cartão. Digite o **endereço** e o **código** do Windows → **Parear com código**.

Resultado esperado no Android: mensagem começando com **"Acervo recebido de"** e contendo
**"1 imagem recebida"**.

📌 **E2** — foto das duas telas: o Windows com endereço e código, o Android com a mensagem.

**Confira no Android:**

- o universo, a história, o livro e o **"Capítulo do teste"** existem;
- a **frase** está igual;
- a **imagem aparece**.

📌 **E3** — foto do capítulo aberto no Android, com texto e imagem.

**O SHA do blob no Android** (com cabo e depuração USB):

```bash
adb shell run-as com.narrahub.app find . -path '*blobs/sha256*' -type f
```

Cada arquivo tem como **nome** o próprio SHA-256. Pegue o caminho que apareceu e confira:

```bash
adb shell run-as com.narrahub.app sha256sum <caminho que apareceu>
```

📌 **E4** — o nome do arquivo e o `sha256sum` são **o mesmo** hash. Se forem diferentes, o gate
reprova: arquivo com nome de um conteúdo e bytes de outro é exatamente o que a etapa 13 existe
para impedir.

Sem cabo, o E4 fica pendente e o gate não fecha — o E3 prova que a imagem aparece, não que os
bytes conferem.

---

## 3. Incremental Android → Windows (B → A)

1. **No Android:** abra o "Capítulo do teste" e **mude a frase**. Salve.
2. **No Windows:** a escuta continua ligada (se desligou, **Escutar nesta rede** de novo; o
   endereço não muda).
3. **No Android:** digite o endereço do Windows → **Sincronizar pareado**. *Sem código* — os dois
   já se conhecem.

Resultado esperado no Android: mensagem começando com **"Sincronizado com"** e contendo
**"1 alteração enviada"**.

**No Windows, saia do capítulo e entre de novo.** A tela não recarrega sozinha depois de uma
sincronização — o dado já está no banco, mas o editor aberto mostra o que tinha carregado. Isso
é limitação conhecida da UI mínima, não falha de convergência.

📌 **E5** — foto do capítulo no Windows mostrando a **frase escrita no Android**.

---

## 4. Incremental Windows → Android (A → B)

1. **No Windows:** mude a frase do capítulo mais uma vez. Salve.
2. **No Android:** **Escutar nesta rede** — agora é o Android que escuta. Anote o endereço.
3. **No Windows:** digite o endereço do **Android** → **Sincronizar pareado**.

Resultado esperado no Windows: mensagem começando com **"Sincronizado com"** e contendo
**"1 alteração enviada"**.

**No Android, saia do capítulo e entre de novo**, pelo mesmo motivo do passo 3.

📌 **E6** — foto do capítulo no Android mostrando a **frase escrita por último no Windows**.

Este passo inverte quem escuta **de propósito**: prova que o Android atende conexão, e não só
que conecta. Uma escuta que só funciona no desktop reprovaria aqui.

---

## 5. Contraprova rápida (opcional, 3 minutos)

Prova negativa de que o código importa:

1. No Windows, **Novo código**. No Android, tente **Parear com código** com um código **errado**.
2. Esperado: falha. Tente mais duas vezes com códigos errados.
3. Agora digite o **código certo** — ainda deve **falhar**: três tentativas erradas matam o código.

📌 **E7** (opcional) — a mensagem de recusa na quarta tentativa.

---

## Veredito

| evidência | o que prova | obrigatória |
| --- | --- | --- |
| E1 | acervo com imagem em A | sim |
| E2 | pareamento por PIN entre sistemas diferentes | sim |
| E3 | bootstrap: texto e imagem aparecem em B | sim |
| E4 | blob físico em B com SHA conferido | sim |
| E5 | incremental B → A converge | sim |
| E6 | incremental A → B converge, com B escutando | sim |
| E7 | o limite de tentativas vale no aparelho real | não |

**Com E1 a E6, a linha "Windows ↔ Android físico" do fechamento da etapa 14 fica ✅.**

## Se algo falhar

Anote **qual passo**, a **mensagem exata** das duas telas, e o **modelo e versão do Android**. As
mensagens foram escritas para dizer onde parou:

| a mensagem contém | onde olhar |
| --- | --- |
| "não pôde ser aberta" | outro programa usando a porta, ou falta de permissão |
| "a conexão com o outro aparelho falhou" | rede isolando clientes, firewall, endereço digitado errado |
| "conectou e não respondeu" | o outro lado travou ou saiu da tela |
| "não chegaram ao mesmo código" | código digitado errado |
| "Este acervo não pode ser enviado agora" | o Windows tem conflito ou pendência de mídia aberta |
| "O acervo recebido não pôde ser gravado" | o Android não estava vazio — reinstale o APK |
| "o aparelho não está pareado com este" | "Sincronizar pareado" antes de parear, ou em aparelhos trocados |
