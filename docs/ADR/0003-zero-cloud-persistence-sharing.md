# ADR 0003 — Compartilhamento sem persistência em nuvem

## Contexto

O produto precisa compartilhar conteúdo temporariamente sem transformar um servidor remoto em fonte de verdade.

## Decisão

Cloudflare Pages hospeda somente o visualizador estático. Quick Tunnel transporta tráfego para o servidor Rust do autor. Pacotes ficam cifrados em memória durante a sessão e contribuições aceitas são persistidas imediatamente no SQLite do autor.

## Consequências

- Não usar Worker, Durable Objects, D1, KV ou R2 para sessões.
- O Quick Tunnel não oferece SLA e deve possuir retry e fallback.
- Ao encerrar, URL, servidor e chaves em memória deixam de existir; histórico local permanece.
