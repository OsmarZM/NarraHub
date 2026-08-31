# NarraHub 0.9.0

Esta versão corrige três regressões visuais introduzidas na 0.8.0 — o fundo cósmico, a logo da barra de título e a troca entre tema claro e escuro — e dá ao quadro de Planejamento a escolha de onde cada propriedade aparece: em todos os cards do universo ou só no card que a criou.

**Minor, e não patch, porque acrescenta funcionalidade compatível e traz uma migration nova.** A 0.8.0 era só código; esta muda o formato do banco.

---

## Correções

### 1. Fundo cósmico e logo voltaram a aparecer

As duas sumiram na 0.8.0. A causa era a mesma: o código passou a pedir arquivos `.webp`, extensão que nunca existiu no projeto. Os arquivos sempre foram `cosmic-nebula.jpg` e `narrahub-logo-full.png`.

### 2. Tema claro e escuro voltou a mudar de verdade

O fundo do aplicativo passou a ser desenhado por variáveis de tema (`--nh-bg-nebula-img`, `--nh-bg-nebula-color`, `--nh-star-opacity`, `--nh-vignette`) que **nunca chegaram a ser definidas em lugar nenhum**. Com o valor de reserva escuro valendo nos dois temas, a nebulosa cobria a tela inteira mesmo no tema claro.

- As quatro variáveis agora existem nos três blocos de tema (`:root`, `[data-theme='dark']` e `prefers-color-scheme`).
- No tema claro o fundo é um pergaminho luminoso, sem estrelas.
- As estrelas somem por `display`, não por `opacity`: as animações de cintilação sobrescrevem `opacity` e venceriam a variável.

### 3. Ficha de entidade abre no começo

Abrir uma entidade que estava no fim da lista mostrava a ficha já rolada até o rodapé — lista e ficha compartilham o mesmo contêiner rolável. A ficha abre no topo, e voltar devolve a lista ao ponto em que você estava.

---

## Novidades

### 4. Planejamento: alcance da propriedade

Até aqui toda propriedade criada no quadro valia para todos os cards do universo, e a tela não dizia isso — você criava um campo dentro de uma ficha e ele aparecia em todas.

- **Escolha ao criar**: em todos os cards, ou só neste card.
- **Mude depois**: cada propriedade tem um botão que a torna universal ou a devolve ao card de origem. Os valores já preenchidos não se perdem em nenhuma das duas direções.
- **A ficha lista só o que vale para ela**: as universais mais as do próprio card.
- **Excluir um card** leva junto, na mesma operação, as propriedades que existiam só nele.

O nome da propriedade continua único dentro do universo. É isso que permite promover um campo de card para universal mexendo só no alcance, sem risco de colidir com um homônimo criado em outro card.

---

## Banco de dados

### Migração v15 (SQLite), aditiva

| Coluna | Papel |
| --- | --- |
| `scope` | `universal` ou `card`. O padrão é `universal`, então **nenhuma propriedade existente muda de comportamento no upgrade** |
| `owner_item_id` | O card dono, quando o alcance é `card` |

Dois gatilhos recusam os dois estados inconsistentes: propriedade de card sem dono e propriedade universal com dono. Nenhuma migration publicada foi alterada.

A regra de visibilidade vale no core Rust, não só na tela: `planning_save_card` carrega apenas as definições universais mais as do próprio card, então um valor endereçado à propriedade privada de outro card é recusado com erro em vez de ignorado em silêncio.

---

## Verificação e qualidade

- **Consistência de versão**: 0.9.0 sincronizada em `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` e `src-tauri/Cargo.lock`.
- **166 testes Rust**, incluindo a cadeia de migrations 1→15 e um upgrade de v14 para v15 com `integrity_check` e `foreign_key_check` limpos.
- **38 testes de arquitetura**, 5 de IA, 4 de planejamento, 4 de compartilhamento, 2 de navegação.
- **`cargo fmt --check` e `cargo clippy -D warnings`** limpos.
- Build Angular de produção sem avisos de orçamento.

### O que esta lista não cobre

Build local, instalador gerado e testes verdes são evidências diferentes de "funciona no computador do escritor". Continua pendente, e é o roteiro de aceitação desta versão:

1. instalar em pasta limpa e abrir;
2. abrir um universo que já tenha propriedades no quadro e confirmar que **todas continuam aparecendo em todas as fichas**;
3. criar uma propriedade "só neste card", fechar e reabrir o app, e confirmar que ela não vazou para os outros cards;
4. conferir o fundo e a logo nos dois temas;
5. abrir uma entidade do fim da lista e confirmar que a ficha abre no topo.

O caminho de atualização pelo próprio aplicativo cria e **valida** um backup `pre_update` antes de baixar qualquer coisa, e interrompe a atualização se o backup não validar.
