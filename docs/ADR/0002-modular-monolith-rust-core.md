# ADR 0002 — Monólito modular com núcleo Rust

## Contexto

O componente Angular raiz concentra navegação, estado e regras, enquanto parte do CRUD ainda executa SQL no frontend. Microserviços aumentariam contratos e pontos de falha sem necessidade operacional.

## Decisão

Manter um único aplicativo distribuível. Angular cuida da experiência e stores; Rust concentra casos de uso transacionais; SQLite permanece a infraestrutura local. A migração usa gateways e adaptadores por feature.

## Consequências

- Não existe reescrita total.
- Legacy SQL e Rust podem coexistir, mas nunca escrever a mesma operação simultaneamente.
- Remoção do acesso SQL do frontend ocorre apenas ao final da migração.
