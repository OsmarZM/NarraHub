# Roteiro físico — experiência mobile no Android

> O que o Chromium dos testes automáticos não emula: teclado real, pinça, gesto voltar do sistema,
> barras e recorte da câmera, vibração e fluidez num aparelho de verdade. Anote as evidências 📌.
> Tempo: 20 a 30 minutos.

## 0. Preparar

1. APK de debug da PR #56 (artefato `narrahub-debug-apk` da última execução verde da CI).
2. Desinstale o NarraHub que estiver no aparelho (debug e release têm assinaturas diferentes).
3. Anote: modelo, versão do Android, navegação por **gestos** ou **3 botões**.

## 1. Primeira abertura

1. Abra o app.
2. A barra de cima fica **abaixo** do relógio e dos ícones de status, nunca por baixo deles.
3. Ao lado da borda direita, na altura do polegar: **três barras douradas** e a dica
   **"← Puxe para navegar"**, que some sozinha em ~5 s.
4. Feche e abra o app: a dica **não** aparece de novo.

📌 **E1** — print da primeira abertura com a dica.

## 2. Zoom

1. Na biblioteca, faça **pinça** com dois dedos: a interface **não** aumenta nem diminui.
2. **Toque duplo** num texto: **não** dá zoom.
3. Rolar com um dedo continua funcionando.

📌 **E2** — anote "sem zoom" (ou o que aconteceu).

## 3. Navegação gestual

| # | ação | esperado |
| --- | --- | --- |
| 3.1 | **tocar** nas três barras | a pilha de cartões abre; vibra leve |
| 3.2 | empurrar a pilha para a **direita** | fecha |
| 3.3 | **arrastar** das barras para a esquerda, devagar | abre acompanhando o dedo |
| 3.4 | com a pilha aberta, arrastar **para cima e para baixo** | os cartões giram como roda de relógio; vibra a cada cartão; assenta num cartão |
| 3.5 | peteleco **curto e rápido** nas barras | abre |
| 3.6 | tocar no cartão da frente | vai para a área |

**Conflito com o "voltar" do Android** (só com navegação por gestos):

| # | ação | esperado |
| --- | --- | --- |
| 3.7 | deslizar da borda **esquerda** | Android volta |
| 3.8 | deslizar da borda **direita**, bem **acima** das barras | Android volta |
| 3.9 | deslizar da borda direita **nas barras** | abre a navegação do NarraHub, não volta |

📌 **E3** — gravação de tela curta de 3.3 e 3.4 (fluidez da roda).

## 4. Universo

1. Crie um universo e abra.
2. Barra de cima: **‹**, nome da área em dourado, nome do universo; **🔍** e **•••**.
3. Nenhuma barra lateral nem linha de botões Renomear/Tags/Compartilhar no topo.
4. Toque em **•••**: folha de baixo com Novo capítulo, Resumo, Renomear universo, Tags, Compartilhar.
5. **‹** volta para a biblioteca.

📌 **E4** — print da folha de ações.

## 5. Escrita e teclado

1. Na História: **☰ Capítulos** abre a folha; crie história, livro e capítulo pela folha.
2. Em cada item da árvore, **⋯** abre Mover, Renomear, Tags, Excluir.
3. Toque no capítulo: a folha fecha e o editor ocupa a tela.
4. Toque no texto: o **teclado abre e o cursor continua visível**; digite três parágrafos.
5. A barra de ferramentas rola para o lado **por dentro**; a página não rola para o lado.
6. Com o teclado aberto, a tela não pula nem dá zoom.
7. **Resumo** abre a folha; digite no resumo com o teclado aberto: o campo continua visível.

📌 **E5** — print do editor com o teclado aberto.

## 6. Diálogos com teclado

1. **•••** → Renomear universo: a caixa sobe de baixo; o teclado abre; **o botão Salvar continua
   visível acima do teclado**.
2. Busca (**🔍**): digite com o teclado aberto.
3. Configurações → qualquer campo: sem zoom ao focar.

📌 **E6** — print do Renomear com teclado.

## 7. Telas especiais

| área | esperado |
| --- | --- |
| Planejamento | uma coluna por vez; deslizar troca de coluna; as pílulas de etapa levam à coluna |
| Timeline | marcos em lista vertical com o eixo à esquerda; ✎ # × tocáveis |
| Conexões | grafo ocupando a tela; ações no **•••** |
| Personagens | fichas em uma coluna; ✎ e × visíveis sem precisar "passar o mouse" |
| Configurações | uma coluna; botões e campos de dedo |

📌 **E7** — print do Planejamento e da Timeline.

## 8. Deitado

Gire o aparelho na biblioteca e na escrita: continua o shell do celular (não vira desktop), sem
conteúdo atrás do recorte da câmera.

## 9. Desktop (Windows) intacto

Abra o NarraHub no Windows: barra de título, sidebar, cabeçalho com botões, árvore e resumo lado a
lado — como antes.

## Veredito

| evidência | prova |
| --- | --- |
| E1 | dica, barra abaixo do status |
| E2 | sem zoom |
| E3 | gestos e fluidez |
| E4 | ações em folha |
| E5 | editor com teclado |
| E6 | diálogo com teclado |
| E7 | telas especiais |
