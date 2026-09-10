# ADR 0010 — Blob store endereçado por conteúdo

```text
Status:   Accepted
Data:     2026-09-10
Fase:     4 — Sync V2, etapa 13
Proposto por: Claude
```

## Contexto

O [ADR 0009 §17](0009-sync-v2.md) decidiu como anexos atravessam o Sync V2: endereçamento por
SHA-256 do conteúdo, arquivos idênticos compartilhando um blob, hash recalculado na chegada, e
transferência bidirecional.

O schema não implementa nada disso. Os bytes vivem **dentro** do SQLite, em base64.

E não é uma coluna: **são dez superfícies**, descobertas uma a uma ao tentar escrever este
documento. A ordem em que apareceram importa, porque cada uma invalidou uma premissa da
anterior — o registro dessa sequência está em [Como as dez foram encontradas](#como-as-dez-foram-encontradas).

O threat model do ADR 0009 §4 continua valendo sem alteração. Este ADR não muda quem confia em
quem; muda onde os bytes moram.

## O problema, em três frases

Um capítulo com uma imagem de 6 MB colada no editor produz `chapters.content` com ~8 MB de
base64. `update_chapter` serializa **o agregado inteiro** como payload, então cada autosave
escreve outro evento de 8 MB no log assinado e append-only — que não se reescreve nunca. O
gatilho `trg_chapter_revision` copia o documento anterior para `chapter_revisions` no mesmo
instante, dobrando o custo dentro do próprio banco.

Isso não é ameaça de adversário. É o desenho cobrando proporcional ao arquivo onde deveria
cobrar proporcional à mudança.

## As dez superfícies

**Campo direto — um campo, zero ou um asset:**

| # | Superfície |
| --- | --- |
| 1 | `attachments.data_url` |
| 2 | `universes.cover_image` |
| 3 | `entities.image` |
| 4 | `canvas_nodes.image` |
| 5 | `books.cover_image` |
| 6 | `planning_items.image` |

**Documento estruturado — um documento, zero a N assets, embutidos em posições arbitrárias
do HTML:**

> **Correção de 2026-09-10 (NH-065).** A versão original desta seção dizia "do JSON do
> Tiptap". Estava errada, e a premissa era minha. O repositório persiste **HTML**:
> `editor.getHTML()` na saída, `setContent(html)` na entrada, e `normalizeIncoming` envelopa
> texto puro legado em `<p>`. O `attrs` do node existe no ProseMirror, em memória; o que chega
> ao SQLite é `<img src="data:image/png;base64,…">`. Vale para as quatro superfícies — a 10
> confirma pelo outro lado, com o convidado editando `content` num `contentEditable`.

| # | Superfície | Escritor |
| --- | --- | --- |
| 7 | `chapters.content` | editor |
| 8 | `chapter_revisions.content` | **gatilho `trg_chapter_revision`** |
| 9 | `sync_conflicts.local_value` + `remote_value`, só onde `aggregate_type = 'chapter' AND field = 'content'` | V1 |
| 10 | `collaboration_contributions.original_value` + `proposed_value`, mesmo recorte | colaboração |

As dez convergem para o **mesmo** blob store, e deduplicam entre si. A origem visual do
arquivo não participa da identidade: a capa de um livro, a foto de uma entidade, um anexo, uma
imagem no capítulo, a mesma imagem numa revisão antiga e os dois lados de um conflito, todos
com os mesmos bytes, são **um** arquivo físico e dez referências.

### Fora da migração, explicitamente

- **`sync_events.payload` histórico.** É assinado, imutável e append-only. Reescrevê-lo
  quebraria assinatura, identidade do evento e as invariantes causais do ADR 0009. A garantia
  desta etapa é *"nenhum evento **novo** carrega binário inline"*, e **não** *"apagar base64 de
  eventos históricos"*. O bootstrap não depende desses eventos, porque a etapa 12 usa snapshot
  mais baseline.
- **Campos semanticamente textuais** — `chapter.summary`, `entity.description`,
  `universe.description`, atributos de ficha, campos de planejamento. Uma string ali pode
  literalmente conter `data:image/png;base64,…` e ainda ser texto legítimo: um personagem pode
  estar explicando o que é uma data URL. Transformar texto parecido com mídia em arquivo seria
  errado. O `CHECK` de `planning_field_definitions.field_type` confirma que não há tipo de
  campo de mídia no planejamento.

## Decisão

**Os bytes de imagem saem do SQLite. O banco guarda referência por `SHA-256(bytes reais)`, e um
único blob store, compartilhado por todas as superfícies, guarda os arquivos.**

## O contrato

1. **O identificador é `SHA-256(bytes reais)`**, hexadecimal minúsculo de 64 caracteres.
2. **O hash nunca é calculado sobre a representação textual.** Não sobre
   `data:image/png;base64,…`, não sobre o base64 isolado, não sobre um path. O mesmo PNG
   gravado com MIME declarado diferente ou quebra de linha diferente no base64 tem que produzir
   o mesmo hash — senão a deduplicação prometida desaparece sem ninguém perceber.
3. **Os bytes ficam fora do SQLite**, sob `app_data/assets/blobs/sha256/<ab>/<hash>`.
4. **Nenhum caminho absoluto é persistido.** O banco guarda o hash; o caminho é derivado dele.
   Caminho gravado quebra na restauração noutra máquina.
5. **Duas referências podem apontar para o mesmo hash.** É o objetivo.
6. **Evento de sync carrega referência e metadado, nunca base64.**
7. **A transferência de blob é separada do log causal.** O cursor não espera arquivo grande.
8. **O hash é recalculado antes de publicar** o arquivo recebido. Diferente do esperado, o blob
   é descartado — a disciplina com que o backup recusa banco adulterado.
9. **Gravação é temporário mais publicação atômica.** Escreve `.part`, verifica, e só então
   `rename`. Arquivo parcial nunca aparece sob o nome de um hash válido.
10. **Referência e presença física são estados distintos.** No incremental, a linha pode existir
    com o blob ausente e a tela mostra o asset como indisponível. No **bootstrap**, não.
11. **Não há GC de blobs nesta etapa.** Apagar referência não apaga arquivo. Blob órfão é
    aceitável; GC distribuído levanta as mesmas perguntas causais dos tombstones (ADR 0009 §15).
12. **O documento nunca controla um path.** `data-narrahub-blob` → validação de formato canônico → o
    resolver produz a URL local. Um hash recebido não pode virar path traversal.

### O formato canônico persistido

Uma imagem blob-safe no HTML guardado é exatamente isto:

```html
<img data-narrahub-blob="<64 hex minúsculos>"
     data-mime-type="image/png"
     alt="rosto.png">
```

`data-` porque é atributo de **dado**, não de imagem: nenhum navegador tenta buscar nada a
partir dele, que é o que se quer de uma referência que só o aplicativo sabe resolver. O `src`
sai do que é persistido.

O que **nunca** vai ao banco:

| Proibido | Por quê |
| --- | --- |
| `data:` URL, base64 | é o problema que esta etapa existe para resolver |
| caminho absoluto | quebra na restauração noutra máquina |
| URL temporária | vence, e o documento fica apontando para nada |
| `blob:` do navegador | morre com a aba |

**O hash é a identidade portátil.** O mesmo HTML tem que funcionar igual no Windows e no
Android, e é isso que a ausência de caminho garante. A resolução `hash → recurso local`
acontece **só em tempo de execução**, e a URL resolvida nunca volta ao documento guardado.

### Transformação das superfícies 7–10

Decisão **A** da NH-065: o HTML continua a representação persistida, e a transformação usa
`lol_html` — um reescritor em streaming feito para trocar um atributo e repassar todo o resto
verbatim. Um varredor artesanal de `<img` foi recusado: tropeça em comentário HTML, em `<img`
dentro de valor de atributo e em entidade escapada, e é o meio-termo entre estrutura e
substring que esta etapa não aceita. Trocar a persistência para JSON também foi recusado —
ampliaria a etapa 13 para uma migração do modelo de documento inteiro e cruzaria com decisões
futuras de identidade por bloco.

Duas passadas, e a separação é decisão:

```text
passada 1   varre e coleta os <img> em ordem de documento   — sem I/O
decisão     decodifica, publica no BlobStore, verifica      — fora do rewriter
passada 2   reescreve o N-ésimo <img> com a decisão N
```

Publicar blob dentro do handler seria I/O de disco no meio de um parser em streaming, com o
erro atravessando `Box<dyn Error>`. Separando, a passada 1 serve de graça aos **guards** — quem
só precisa perguntar "este documento ainda tem mídia inline?" nunca toca o disco.

Preserva `alt`, `title`, dimensões e **todo atributo que não seja o binário**, inclusive os
desconhecidos, porque o rewriter parte do elemento real em vez de reconstruí-lo. E um documento
sem imagem nenhuma sai **byte a byte igual**: `converter` devolve a entrada sem passar pelo
rewriter quando não há nada a trocar, para o acervo inteiro do escritor não ser reescrito e
normalizado por acidente.

**Um transformador só**, reutilizado pelas quatro superfícies. Quatro parsers seriam quatro
interpretações do mesmo formato, envelhecendo em quatro velocidades.

O guard que impede regressão **valida estrutura, não substring**: procura um elemento `img`
cujo `src` comece em `data:`, e valida o formato novo exigindo `data-narrahub-blob` canônico.
`content.contains("data:")` encontraria texto legítimo do escritor — e
`&lt;img src="data:…"&gt;` escrito por um personagem é texto, não elemento.

**Consequência para o editor:** a extensão atual declara
`parseHTML() { return [{ tag: 'img[src]' }] }`, e o seletor **exige** `src`. Um documento novo
não tem, então o Tiptap descartaria o elemento ao carregar. `parseHTML` precisa virar
`tag: 'img'` com os atributos de dado declarados. Isso não é redesenho do editor nem mudança
de UX — é a mesma extensão lendo o formato novo.

### Política do legado inválido

```text
data URL válida    →  migra, grava hash e MIME, verifica, limpa o inline
inválida ou desconhecida  →  preserva o valor EXATO, hash fica vazio,
                             registra issue, não destrói nada
```

**Falha de migração não transforma dado desconhecido em ausência.** Nada de baixar URL, abrir
path, ou interpretar string arbitrária como arquivo — sem heurística.

Nas superfícies de dois lados (9 e 10), a issue identifica **qual** lado falhou. Falha num lado
não autoriza tocar no outro.

### Três barreiras para dado novo

Migrar o legado resolve o passado. O que fecha a torneira são as barreiras na entrada:

1. **Inserção no editor** nasce em hash: `File → bytes → BlobStore.put → node com hash`. Base64
   pode atravessar o IPC como detalhe de transporte; o que é proibido é **persistir**.
2. **`store_contribution`** normaliza `chapter/content` na fronteira, antes do `INSERT`. Se a
   transformação falhar, erro controlado — sem fallback para inline. Dado novo **não** ganha
   migration issue: issue é para legado já existente.
3. **`review(approved)` e `update_chapter`** validam fail-closed. Uma proposta ou um documento
   que ainda contenha mídia inline não é aprovado nem gravado; a contribuição continua
   `pending`, o capítulo não muda, nenhuma revisão é criada, nenhum evento recebe bytes.

A terceira barreira parece redundante depois da segunda, e não é: ela cobre legado não migrado,
banco importado, bug de versão antiga, contribuição adulterada e regressão futura no caminho de
entrada.

## Backup: reutilizar, não construir

`database/backup.rs` já varre `app_data/assets/` recursivamente, **recusa symlink**, e grava
`path`, `sha256` e `size_bytes` num `BackupAssetsManifest` com hash próprio. `validate_assets`
compara disco contra manifesto; o restore em `database/recovery.rs` prepara em staging, recusa
divergência e troca com pontos de rollback.

**Pôr os blobs sob `app_data/assets/blobs/` os coloca nessa cobertura sem uma linha de código de
backup nova.** Isso é decisão, não detalhe: um store em outro lugar exigiria um segundo
mecanismo com um segundo manifesto para manter em dia, e dois mecanismos de backup significam
que um deles vai envelhecer sem ninguém perceber. Mover bytes para fora do SQLite sem preservar
restauração seria P0.

## Migração: o inline não morre agora, mas fica vazio

A conversão não cabe em SQL — exige parse de MIME, decodificação, SHA-256, escrita e `rename`.

```text
migration 20   acrescenta referência de blob, mantém as colunas antigas
backfill Rust  idempotente, sobrevive a morrer no meio
migration N    remove as colunas, e só com prova executável de que não há legado
```

**A coluna antiga permanece no schema; o valor dela não.** Depois da migração bem-sucedida o
campo inline fica vazio. Manter `blob_hash = ABC` ao lado de 4 MB de base64 permanentemente não
resolveria nada: o SQLite continuaria gigante, o bundle continuaria carregando os bytes porque a
etapa 12 copia a linha, e uma NH-053 futura poderia serializar a coluna antiga por acidente.

O backfill lida com quatro estados, e o quarto é o que exige cuidado:

| hash | inline | ação |
| --- | --- | --- |
| vazio | vazio | nada a migrar |
| vazio | válido | migra |
| vazio | inválido | preserva, registra issue |
| preenchido | preenchido | **verifica o blob físico** antes de limpar; se ausente e o inline for válido, reconstrói; se ausente e inválido, preserva e mantém a issue |

**Nunca limpar os bytes antigos antes de haver blob válido publicado.** Enquanto o hash estiver
vazio, o inline é a única cópia daquele arquivo.

As issues ficam numa representação central, e não espalhadas em dez tabelas — dez cópias da
mesma regra envelheceriam em dez velocidades. Precisa responder programaticamente: quantos
assets faltam, quais, e por quê.

## Bootstrap

O conjunto de hashes necessários vem de **todas** as referências: as seis colunas mais os
hashes encontrados **dentro** dos documentos transferidos. É um conjunto único — a mesma imagem
referenciada quarenta vezes é transferida uma vez.

```text
1. capturar snapshot + extrair o conjunto de hashes
2. receptor calcula o que falta
3. transferir
4. verificar SHA-256
5. só então semear o banco
```

A ordem inversa produz o estado ruim: banco commitado com quarenta referências e a transferência
morta no blob 3. Falhar antes do seed deixa, no pior caso, blobs órfãos — inofensivos.

**Asset legado não migrável bloqueia o bootstrap**, com erro nomeando quais. Um aparelho novo
não pode nascer fingindo que recebeu estado completo, pelo mesmo princípio que já recusa
divergência V2 e conflito V1 pendentes.

**Mas não bloqueia o aplicativo.** O escritor continua abrindo o acervo, editando texto,
acessando o resto, e substituindo ou removendo a imagem problemática. Só operações cuja garantia
depende de todos os assets serem representáveis são impedidas: bootstrap, remoção definitiva das
colunas legadas, e qualquer coisa que declare a migração concluída. Sync incremental de conteúdo
não relacionado não trava.

### Reclassificação na matriz da etapa 12

| Tabela | Antes | Depois | Condição |
| --- | --- | --- | --- |
| `attachments` | `EtapaPosterior` | `TransferidaNoBundle` | só quando a linha carregar hash, não base64 |
| `chapter_revisions` | `BloqueiaBootstrap` | `TransferidaNoBundle` | depois de blob-safe; semeada após `chapters`, pela FK |
| `sync_conflicts` | `BloqueiaBootstrap` | **continua** | conflito V1 aberto segue recusando a captura |

Um gate impede que `attachments` seja marcada como transferível enquanto existir caminho normal
gravando `data_url` sem `blob_hash`.

O bootstrap passa a preservar o histórico de revisões do doador. Isso **não** eventiza
`chapter_revisions`: a semântica distribuída daquele histórico continua separada, e o fato de o
gatilho criar revisões localmente em cada peer levanta uma pergunta de identidade e convergência
que fica registrada como dívida — não é NH-053, e não é esta etapa.

## Como as dez foram encontradas

Registrado porque cada passo corrigiu uma premissa que parecia segura, e a sequência é o
argumento para o gate de catálogo:

1. A análise inicial olhou `attachments` e concluiu "uma coluna".
2. Uma varredura por nome de coluna em cinco tabelas escolhidas a dedo achou quatro.
3. Varrer as **36** tabelas pelo `PRAGMA` achou seis — `books.cover_image` e
   `planning_items.image` estavam fora porque as tabelas não tinham sido consultadas.
4. Seguir o **dado** em vez do nome — os call sites de `fileToDataUrl` — achou a sétima, dentro
   de `chapters.content`. Essa é a que já estava ativa: `chapter` é o único agregado que emite
   evento hoje.
<!-- chapter-revisions:premissa-corrigida -->
5. **A premissa mais errada:** eu afirmei que `chapter_revisions` era tabela morta, porque
   nenhum Rust ou TypeScript escreve nela. Quem escreve é o **banco** —
   `trg_chapter_revision`, `BEFORE UPDATE OF content, title ON chapters`, vivo no schema final.
   Buscar escritores em código-fonte não encontra escritores em SQL.
6. `sync_conflicts` guarda o corpo do capítulo duas vezes, e não tem caminho de resolução —
   o `resolved_at` existe no schema para uma resolução que nunca foi escrita.
7. `collaboration_contributions` guarda duas vezes **e tem caminho de aplicação vivo**:
   `writable_column("chapter", "content")` escreve `proposed_value` direto em
   `chapters.content`. Uma proposta legada aprovada depois da migração reinjetaria base64 num
   capítulo já migrado, e o gatilho copiaria para as revisões. Foi o que exigiu a terceira
   barreira.

O gate de catálogo de superfícies binárias existe por causa desta lista: a próxima coluna de
asset inline precisa reprovar a arquitetura, não ser descoberta meses depois.

## Consequências

**Passa a ser possível:** anexar a mesma imagem em dez lugares pagando um arquivo; sincronizar
capítulo ilustrado com custo proporcional ao texto; semear um aparelho novo com anexos e
histórico; e ter backup de blob sem escrever backup de blob.

**Passa a ser proibido:** guardar base64 em coluna nova; gravar caminho absoluto no banco;
publicar blob sem verificar hash; limpar o inline antes de haver blob válido; aprovar proposta
com mídia inline; e fazer o cursor causal esperar transferência de arquivo.

**Fica pior:** um asset passa a ter dois lugares onde pode faltar — a referência e o arquivo — e
a interface precisa dizer "ainda baixando" em vez de "não existe". E o editor ganha uma camada
de resolução entre o documento persistido e o que o WebView renderiza.

## Fora do escopo

Chunking (ADR 0009 §17 já o exclui da V2) · GC de blobs · nuvem, S3, CDN, upload remoto
(contrariam o [ADR 0003](0003-zero-cloud-persistence-sharing.md)) · criptografia at-rest
(threat model próprio) · remoção definitiva das colunas legadas · eventização de universo,
entidade, canvas, livro, planejamento e revisões, que continua sendo **NH-053** · redesign do
editor: muda a representação persistida do node de imagem, não a experiência de inserir, a
posição, nem o layout.

**Dívida registrada nesta análise, fora desta etapa:** `IncomingContribution.original_value` e
`proposed_value` não têm limite de tamanho — o único `MAX_` da colaboração é
`MAX_ATTRIBUTE_KEY = 120`, que cobre a chave do atributo. É questão de limite de entrada, não de
blob store, e merece hardening próprio.
