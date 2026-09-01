# Checklist de release desktop

Este documento é **gate**, não recomendação. Item em branco conta como não feito.

Ele existe porque as três frases da Regra de publicação em
[`ARCHITECTURE_EVOLUTION_PLAN.md`](ARCHITECTURE_EVOLUTION_PLAN.md) continuam valendo:

```text
COMPILA        != FUNCIONA
FUNCIONA       != ESTÁ SEGURO
ESTÁ SEGURO    != É PUBLICÁVEL
```

---

## 1. O que já é automático

Não repita nada disto à mão. Se quebrar, quebra sozinho.

### Em todo Pull Request (`.github/workflows/ci.yml`)

| Verificação | Onde |
| --- | --- |
| Versão consistente nos manifests, README e CHANGELOG | `release:validate-version` |
| Build Angular | `npm run build` |
| Fronteiras do frontend, rotas, contrato do core, rolagem das páginas | `npm run test:architecture` |
| Planejamento e prompts de IA | `test:planning`, `test:ai` |
| API de compartilhamento | `share-api:test` |
| Formatação, clippy e a suíte inteira do core Rust | `cargo fmt`, `cargo clippy -D warnings`, `cargo test` |

Dentro do `cargo test` estão as redes que mais importam numa release, e que também não
precisam de conferência manual: cadeia de migrations com `integrity_check`, fixtures de
schema 10 e 15, backup com WAL, restauração com rollback — inclusive quando o próprio
rollback falha — e o mapa das invariantes de domínio.

### No workflow de release (`release-windows.yml`)

Além de tudo acima: segredos do updater, checksum do `cloudflared`, UI de produção sem
`media="print"` nem `onload` inline, e configuração desktop com os identificadores certos
por perfil.

### Antes de disparar, rode o mesmo conjunto localmente

```bash
npm run release:preflight
```

É exatamente a lista que o job de release executa. Preflight verde não garante a release,
mas preflight vermelho garante que ela vai falhar — e falhar em 4 minutos aqui é melhor que
em 20 no runner.

> Nesta máquina, o `cargo test` precisa dos temporários no `D:` — ver a seção de ambiente em
> [`ai/PROJECT_STATE.md`](ai/PROJECT_STATE.md).

---

## 2. O que só humano consegue

Nenhum destes sobe em teste unitário. É aqui que o checklist ganha o nome.

A ordem importa: parte só pode ser verificada num instalador que já existe, e parte só
depois que a release está no ar. Tratar as duas como uma lista só tornaria o gate impossível
de cumprir — não haveria como "instalar limpo" uma release que ainda não foi construída.

Copie as tabelas para `docs/releases/<versão>.md` e preencha. Uma release sem a tabela 2.1
preenchida não é publicável; sem a 2.2, não é **anunciável**.

### 2.1 Antes de publicar — sobre um instalador local

Gere com `npm run desktop:build` e teste esse artefato. É o mesmo binário que o workflow vai
produzir, com a diferença da assinatura de release.

| # | Verificação | Resultado | Data |
| --- | --- | --- | --- |
| 1 | Instalação limpa numa máquina sem NarraHub | | |
| 2 | Instalador por cima de uma instalação existente | | |
| 3 | Banco antigo migra e reabre com o conteúdo intacto | | |
| 4 | Schema, `integrity_check` e `foreign_key_check` na tela de Ajustes | | |
| 5 | Segundo boot: migration não reaplica | | |
| 6 | Backup criado e validado | | |
| 7 | Restauração de backup devolve o estado esperado | | |
| 8 | Autosave: sair da Escrita salva o capítulo | | |
| 9 | Compartilhamento: sessão abre, link funciona, encerra ao fechar | | |
| 10 | Tema claro | | |
| 11 | Tema escuro | | |
| 12 | Janela em **1366×768**: nada de conteúdo inalcançável | | |

Os itens 3, 4 e 5 têm roteiro próprio em
[`QUALIFICATION_UPGRADE.md`](QUALIFICATION_UPGRADE.md), inclusive qual par de versões
escolher — o que importa é **cruzar migration**, não pegar a mais recente.

O item 12 está na lista porque já falhou: em 1366×768 a lista de backups dos Ajustes ficava
cortada e sem rolagem. Existe teste de regressão para a causa, mas ele verifica a cascata do
CSS, não a janela.

### 2.2 Depois de publicar — sobre a release no ar

| # | Verificação | Resultado | Data |
| --- | --- | --- | --- |
| 13 | Artefatos, assinaturas e `latest.json` presentes no destino público | | |
| 14 | Updater detecta a nova versão a partir da anterior instalada | | |
| 15 | Upgrade pelo updater preserva o conteúdo | | |
| 16 | Nota da versão publicada no `CHANGELOG.md` | | |

Enquanto a 2.2 não estiver verde, a release existe mas **não deve ser anunciada**. Se algo
aqui falhar, o caminho é publicar uma correção — não deixar como está esperando ninguém
reparar.

## 3. Regras que não estão na tabela

**Evidência é por release, não por hábito.** "Sempre funcionou" não preenche linha. Uma
verificação sem data e sem resultado conta como não feita.

**Ambiente alheio vale mais que o do autor.** A máquina de quem desenvolve carrega histórico
de instalações, perfis e configuração que nenhum usuário tem. Sempre que possível, os itens
1 a 3 da tabela 2.1 rodam em outra máquina ou numa VM.

**Nunca instale uma versão mais antiga que o banco em uso** para testar downgrade fora de um
ambiente descartável. O aplicativo recusa o banco por segurança — e a partir da versão que
contém o ADR 0007 ele explica isso numa tela em vez de não abrir.

**Falha na tabela 2.1 impede a publicação.** Não existe "passou quase tudo". Se um item não
se aplica àquela release, escreva por que — não deixe em branco.

---

## 4. Por que "verde no workflow" não basta

Build local, instalador gerado, push no Git e release remota são **evidências diferentes**.
Um workflow verde prova que o empacotamento terminou; não prova que o artefato chegou ao
destino público, nem que um cliente real consegue enxergá-lo.

É por isso que a tabela 2.2 existe separada, e é por isso que o item 14 fala em detectar a
atualização **a partir da versão anterior instalada** — a única forma de saber que o
`latest.json` está onde o updater procura.
