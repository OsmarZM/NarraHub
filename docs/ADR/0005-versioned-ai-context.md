# ADR 0005 — Contexto de IA limitado e versionado

## Contexto

Modelos locais possuem janelas e velocidades diferentes. Enviar cânone e capítulo sem orçamento pode descartar a instrução, gerar eco ou exceder o timeout.

## Decisão

Toda geração usa um contrato de contexto versionado, com tarefa preservada, orçamento por provedor e prioridade para o trecho em edição. Eco, timeout e resposta inválida são falhas explícitas. Conteúdo só é aplicado após confirmação.

## Consequências

- O modelo não consulta o banco diretamente.
- Context Engine escolhe e compacta informações.
- Resumos longos usam blocos e síntese.
- Testes editoriais reais fazem parte do gate de release.
