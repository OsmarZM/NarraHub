# ADR 0001 — Local Ownership

## Contexto

O NarraHub guarda manuscritos e universos que podem representar anos de trabalho. Dependência de conta, internet ou banco central criaria risco de indisponibilidade e perda de controle.

## Decisão

SQLite e arquivos do dispositivo são a fonte canônica. Serviços externos podem transportar conexões ou entregar código público, mas não persistem conteúdo narrativo, sessões ou contribuições.

## Consequências

- O aplicativo funciona offline.
- Backup e recuperação local são infraestrutura central.
- Recursos online degradam graciosamente.
- Cloudflare pode observar metadados de transporte, mas não recebe a chave do conteúdo.
