# Qualificação do ciclo de atualização

Roteiro reproduzível para provar que uma versão nova do NarraHub abre o banco da versão
anterior sem perder nada. É o gate que teste unitário **não** fecha: ele exercita o
instalador, o updater assinado e o reinício real do aplicativo.

Complementa [`PHASE_0_1_QUALIFICATION.md`](PHASE_0_1_QUALIFICATION.md), que registra a
qualificação manual feita na 0.7.4.

---

## 1. O que já é automático, e o que exige humano

A distinção importa: automatizar o que não dá gera teste verde que não prova nada.

### Já roda no CI, a cada PR

| Coberto | Onde |
| --- | --- |
| Cadeia de migrations 1→15 em arquivo real, com `integrity_check` | `full_migration_chain_creates_a_reopenable_file_database` |
| Upgrade de fixture povoada de schema 10 sem perda de dados | `representative_schema10_fixture_upgrades_without_data_loss` |
| Cada migration de v7 a v15, várias com `foreign_key_check` | `database/migrations.rs` |
| Propriedades de planejamento anteriores à v15 seguem universais | `upgrade_para_v15_mantem_campos_existentes_universais` |
| Backup com WAL, hash divergente, manifesto malicioso, staging interrompido | `database/backup.rs` |
| Restauração que falha no meio devolve o banco anterior | `database/recovery.rs` |

**Não repita nada disso à mão.** Se um desses quebrar, quebra no CI antes de chegar aqui.

### Só humano consegue

| Não coberto | Por quê |
| --- | --- |
| O instalador NSIS/MSI substituindo uma instalação existente | Nenhum teste executa o instalador |
| O updater achar `latest.json`, validar a assinatura e aplicar | Depende de release publicada e de rede |
| O app reabrir e reencontrar o conteúdo após reinício real | O runtime Tauri não sobe em teste unitário |
| A aparência depois da atualização | Geometria e tema exigem olho |

Este roteiro cobre a segunda coluna. Nada mais.

---

## 2. Por que 0.8.0 → 0.9.1, e não a versão mais recente

O que interessa num upgrade é **cruzar uma migration**. Comparando os schemas publicados:

| Versão | Schema |
| --- | --- |
| 0.7.6 | 14 |
| 0.8.0 | 14 |
| 0.9.0 e 0.9.1 | **15** |

Uma 0.9.2 publicada hoje teria schema 15, igual à 0.9.1: o upgrade não cruzaria migration
nenhuma e provaria apenas que o instalador não apagou o banco. **0.8.0 → 0.9.1 cruza a
migration 15**, a do alcance de campos do planejamento — a que mais mexe em dado de usuário
até hoje.

As duas já estão publicadas com instalador, assinatura e `latest.json`. **Este roteiro não
exige publicar release nova.**

Quando existir uma migration 16, o par a testar passa a ser `0.9.1 → a versão que a
contiver`, pela mesma lógica: escolha sempre o par que cruza migration.

---

## 3. Ambiente

> **Nunca rode este roteiro no perfil de uso diário.** O passo 1 instala uma versão **mais
> antiga** que o banco em uso. Um executável que só conhece o schema 14 abrindo um banco 15
> é exatamente o cenário que o [ADR 0004](ADR/0004-immutable-migrations-and-updates.md)
> existe para impedir, e o app recusa esse banco por segurança.

Máquina virtual Windows limpa, com snapshot antes de cada fase. O snapshot é o que torna o
roteiro repetível: falhou no passo 5, volta ao snapshot B e repete sem reinstalar tudo.

```text
Snapshot A   VM limpa, sem NarraHub
Snapshot B   0.8.0 instalada e com conteúdo
Snapshot C   depois do upgrade
```

---

## 4. Roteiro

### Passo 1 — Instalar a versão antiga

Instale `NarraHub_0.8.0_x64-setup.exe`, do release `app-v0.8.0`.

Abra **Ajustes → Backup e integridade** e registre:

```text
Schema         deve ser v14
SQLite         deve ser ok
Foreign keys   deve ser 0
```

### Passo 2 — Criar conteúdo que a migration vai tocar

O conteúdo não é decorativo: ele precisa exercitar **o que a migration 15 altera**. Antes da
v15 toda propriedade de planejamento valia para o universo inteiro, e a migration não pode
mudar isso em silêncio.

Crie, no mínimo:

- um universo, uma história, um livro e **dois capítulos com texto**;
- **três entidades** de tipos diferentes, uma com imagem principal;
- uma **relação** entre duas entidades;
- **duas tags** aplicadas a conteúdos diferentes;
- um **evento de timeline** ligado a uma entidade;
- **dois cards de planejamento** e, neles, **três propriedades personalizadas com valor
  preenchido** — este é o item que a migration 15 migra;
- uma **anotação no canvas de Conexões**, que veio na migration 14.

Anote as contagens. Elas são a evidência a comparar depois.

### Passo 3 — Fechar o app pela janela

Pelo X, não pelo Gerenciador de Tarefas: o encerramento limpo faz parte do que se está
testando, porque é ele que dispara o autosave e o fechamento do pool SQLite.

**Tire o snapshot B.**

### Passo 4 — Atualizar

Duas variantes. Rode as duas, cada uma a partir do snapshot B.

**4a — Updater dentro do app.** Abra a 0.8.0 e deixe que ele ofereça a atualização. É o
caminho que a maioria dos usuários vai percorrer, e o único que exercita `latest.json`, a
verificação de assinatura e o download.

**4b — Instalador por cima.** Rode `NarraHub_0.9.1_x64-setup.exe` sobre a instalação
existente. É o caminho de quem baixa manualmente.

### Passo 5 — Reabrir e conferir

Em **Ajustes → Backup e integridade**:

```text
Schema         deve ser v15
SQLite         deve ser ok
Foreign keys   deve ser 0
```

E percorra o conteúdo do passo 2:

- [ ] os dois capítulos abrem com o **mesmo texto**, sem truncar;
- [ ] as três entidades existem, com atributos e a imagem principal;
- [ ] a relação continua aparecendo nas duas fichas;
- [ ] as tags continuam aplicadas aos mesmos conteúdos;
- [ ] o evento de timeline continua ligado à mesma entidade;
- [ ] **as três propriedades de planejamento aparecem em todos os cards do universo** — é
      isso que a migration 15 promete — e **com os valores preenchidos preservados**;
- [ ] a anotação do canvas continua no lugar e continua tracejada, ou seja, segue sendo
      anotação e não virou relação canônica;
- [ ] o tema escolhido antes do upgrade foi preservado.

### Passo 6 — Reiniciar

Feche e abra o app de novo. Migration aplicada uma vez não pode reaplicar nem falhar no
segundo boot. Confira o schema outra vez.

### Passo 7 — Backup e restauração depois do upgrade

Ainda em **Ajustes → Backup e integridade**: crie um backup, use **Validar**, e então
restaure o backup **pré-atualização** que o app criou sozinho. O conteúdo deve voltar ao
estado do passo 2.

Depois restaure o backup criado agora, para voltar ao estado atualizado.

**Tire o snapshot C.**

---

## 5. Registro de evidência

Uma execução só conta se tiver registro. Copie esta tabela para o documento da release:

| Passo | Esperado | Obtido | Data |
| --- | --- | --- | --- |
| 1 · schema inicial | v14 | | |
| 1 · integridade | ok, 0 FK | | |
| 4a · updater | encontra e aplica a 0.9.1 | | |
| 4b · instalador | aplica sobre a instalação existente | | |
| 5 · schema final | v15 | | |
| 5 · integridade | ok, 0 FK | | |
| 5 · conteúdo | tudo da lista do passo 5 | | |
| 5 · campos de planejamento | universais, valores preservados | | |
| 6 · segundo boot | schema v15, sem reaplicar | | |
| 7 · backup e restore | volta ao estado do passo 2 | | |

Falha em qualquer linha impede a publicação. Ver a **Regra de publicação** em
[`ARCHITECTURE_EVOLUTION_PLAN.md`](ARCHITECTURE_EVOLUTION_PLAN.md).

---

## 6. Limites conhecidos deste roteiro

- **É manual.** Automatizá-lo exigiria dirigir o instalador do Windows e a janela do Tauri.
  Enquanto isso não existir, é honesto chamá-lo de checklist e não de teste.
- **Cobre um par de versões por execução.** A matriz completa de migração é trabalho da
  Fase 7.
- **Não cobre Android.** O ciclo de atualização lá é outro e merece roteiro próprio.
- **Não cobre a falha do updater** — rede caindo no meio do download, assinatura inválida,
  `latest.json` corrompido. Merece tarefa separada.
