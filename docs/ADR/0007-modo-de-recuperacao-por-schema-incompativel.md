# ADR 0007 — Modo de recuperação por schema incompatível

```text
Status:       Accepted  (aprovado pelo autor em 2026-09-01)
Data:         2026-09-01
Fase:         1 — Qualification e segurança de atualização
Proposto por: Claude, a partir de um incidente real relatado pelo autor
```

## Contexto

O [ADR 0004](0004-immutable-migrations-and-updates.md) estabelece que migrations publicadas
são imutáveis e que abrir um banco novo com executável antigo **não** é estratégia de
rollback. A consequência prática nunca tinha sido vivida: o que o app faz quando isso
acontece mesmo assim.

**Incidente de 2026-09-01.** O autor desinstalou a 0.9.1 e instalou a 0.8.0. Desinstalar
não apaga `%APPDATA%`, então a 0.8.0 encontrou o banco em schema 15 conhecendo apenas o 14.

O que aconteceu: **o aplicativo não abriu.** Sem janela, sem mensagem, sem instrução.

O que era verdade ao mesmo tempo: o banco estava perfeito — schema 15, `integrity_check`
ok, 4 universos e 5 capítulos, mais dois backups íntegros na pasta do perfil.

O dado nunca esteve em risco. **A percepção de perda foi total.** Essa distância entre o
estado real e o que o usuário consegue saber é o problema que este ADR resolve.

O código já captura a falha de inicialização e tem um alerta pronto no `root-layout` para
mostrá-la. Ele não chegou ao usuário: a falha acontece no `provideAppInitializer`, antes de
a interface existir. Um tratamento de erro que só funciona depois que a aplicação subiu não
serve para o erro que impede a aplicação de subir.

## Opções consideradas

### 1. Não fazer nada

O usuário fica com um app que não abre e nenhuma pista. Backup existe, mas ele precisaria
saber que existe, onde está e o que fazer com ele. Descartada: é a situação atual, e ela
falhou com o próprio autor do produto — alguém que conhece o sistema inteiro.

### 2. Down-migrations (SQL reverso por migration)

O caminho que parece óbvio: cada migration ganha o seu inverso, e o app sabe descer.

Rejeitada, por três motivos.

**Reverter apaga dado do usuário.** A migration 15 acrescentou `custom_field_values` e
`image` em `planning_items`. Descer da 15 para a 14 significa **jogar fora** o que o
escritor preencheu nesses campos. Um downgrade que apaga em silêncio é pior que downgrade
nenhum, porque parece seguro.

**Dobra a matriz de testes para sempre.** Cada migration nova passaria a exigir ida, volta,
e ida-e-volta preservando dado. O custo não é o SQL reverso; é sustentá-lo correto por
anos.

**Resolve o problema errado.** Quem não gosta de uma versão quer o **aplicativo** anterior,
não o **formato de dado** anterior. Tratar os dois como a mesma coisa é o que cria a
armadilha.

### 3. Modo de recuperação antes de abrir o banco

O app verifica o schema do disco **antes** de abrir o pool. Se o banco for mais novo do que
ele entende, não tenta abrir: entra num modo de recuperação que explica a situação e oferece
saídas.

## Decisão

Adotar a opção 3. E definir que **downgrade de dados é restauração do backup
pré-atualização** — nunca SQL reverso.

O NarraHub já cria esse backup sozinho antes de cada atualização; no incidente ele estava
lá, rotulado "Antes de atualizar · Schema 10 · app 0.7.4". O mecanismo de downgrade já
existe. O que falta é o usuário conseguir alcançá-lo quando o app não abre.

### O modo de recuperação

Ao iniciar, antes de `Database.load`, o app compara o schema do disco com o seu
`LATEST_SCHEMA_VERSION`. As peças já existem: o comando `database_health` usa
`inspect_database`, que abre o arquivo com `rusqlite` **independente do pool**, e devolve
`schemaVersion`.

Se `schema_do_disco > schema_do_app`, o pool não é aberto e a tela mostra:

```text
Este NarraHub é mais antigo que os seus dados.

  dados no disco   schema 15
  este aplicativo  entende até 14

Seus dados estão intactos. Este aplicativo é que não sabe lê-los.

  [ Atualizar o NarraHub ]        ← ação primária
  [ Restaurar um backup compatível ]
  [ Abrir a pasta de dados ]
```

Três regras para essa tela:

1. **Nunca abrir o pool.** Nenhuma escrita num banco que este executável não entende.
2. **"Atualizar" é a ação primária**, e usa o updater já existente — a 0.8.0 encontra a
   0.9.1 pelo `latest.json`, como o teste de 2026-08-31 comprovou.
3. **A lista de backups só oferece os compatíveis**, filtrados por
   `backup.schemaVersion <= LATEST_SCHEMA_VERSION` do app. O manifesto de backup já carrega
   esse campo, e a tela de Ajustes já o exibe. Oferecer um backup que também não abre seria
   repetir o problema dentro da solução.

### O downgrade que passamos a oferecer

Voltar para uma versão anterior é uma sequência honesta, não uma mágica:

```text
instalar a versão anterior
        ↓
ela detecta o schema mais novo e entra em recuperação
        ↓
restaurar o backup "Antes de atualizar"
        ↓
o banco volta ao formato que aquela versão entende
```

O que se perde nesse caminho é **o trabalho feito depois da atualização** — inevitável, e
por isso a tela precisa dizer isso com todas as letras, com a data do backup, antes de
confirmar. Perder trabalho sabendo é aceitável; perder sem saber, não.

## Consequências

**Passa a ser possível:** desinstalar uma versão e instalar outra mais antiga sem ficar com
um app morto. O usuário sempre tem uma tela que explica e oferece saída.

**Passa a ser proibido:** abrir o pool sem verificar o schema antes. Vira invariante de
boot.

**Precisa ser migrado:** a verificação tem que existir **na versão nova**, e só protege
downgrades a partir dela. Um usuário que voltar para a 0.8.0 continuará sem tela de
recuperação, porque a 0.8.0 já está publicada e é imutável. Não há como consertar o
passado — só parar de produzir esse futuro.

**Gate que impede a regressão:** teste que abre um banco de schema `LATEST + 1` e prova que
(a) o pool não é aberto, (b) o modo de recuperação é ativado, e (c) a lista de backups
oferecidos exclui os incompatíveis. Some-se a isso um passo no
[`QUALIFICATION_UPGRADE.md`](../QUALIFICATION_UPGRADE.md) cobrindo o downgrade, que hoje
o roteiro só sabe proibir.

## Implementação

Entregue em `NH-015`, na mesma data da aprovação.

O portão é o comando `database_compatibility`, separado de `database_health` por dois
motivos que só aparecem num caminho de boot: ele não roda `integrity_check`,
`foreign_key_check` nem as consultas de invariante — caras num banco de vários MB — e ele
**não falha quando o banco ainda não existe**, que é o primeiro boot de toda instalação
nova. Ele abre o arquivo em `SQLITE_OPEN_READ_ONLY`, então não há como corromper aquilo que
se está tentando proteger.

Uma regra de desenho que vale registrar: **incompatibilidade volta como dado, nunca como
`Err`**. Um portão que estoura devolveria o app exatamente ao defeito que ele existe para
corrigir — morrer sem dizer nada. Há teste só para isso.

Gates que impedem a regressão:

- `banco_mais_novo_que_o_app_e_incompativel` e `incompatibilidade_e_resposta_e_nao_erro`, no
  Rust, com um banco em `LATEST + 1`;
- `o pool só é aberto depois do portão de compatibilidade de schema (ADR 0007)`, em
  `tests/frontend-boundaries.test.mjs`, que compara a posição das duas chamadas no
  `AppBootstrapService` e reprova se a ordem se inverter.

## Revisitar quando

Se algum dia uma migration precisar ser revertida **sem** perda de dado — por exemplo, uma
puramente aditiva, cujo inverso é remover colunas vazias. Aí vale reabrir a opção 2 para
esse subconjunto, sempre com o backup como caminho padrão e o SQL reverso como exceção
justificada.
