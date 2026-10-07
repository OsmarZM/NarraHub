# NH-087 — preparação da beta.16

Data: 2026-10-07. Responsável: Codex. Base: mobile-shell.

## Integração confirmada

PR #82 MERGED em f0d43cd18100f62ebc508b61a99c2f147af22b9b.
Head validado a684e1d14fc9f9b11bc589ec7f38a79d3bdd8338, CI 37613549364:
Angular, Mobile (Playwright), Core Rust e Android SUCCESS. Rust 906 PASS/3 ignorados,
Playwright 182 PASS/17 não aplicáveis; fmt/clippy e APK Android PASS no CI.

Local: build e arquitetura 105 PASS; portátil 7, recuperação 10 e destino 4 PASS.
Interface final: 29 PASS e timeout do primeiro startup; reteste isolado 1 PASS, sem mudança de código.
Compilações locais suplementares não são confundidas com o CI ou com execução física.

## Preparação

Manifests, locks, README, CHANGELOG e versão corrente alinhados para 0.10.0-beta.16.
Última pré-release publicada permanece beta.15 até a publicação verificada da beta.16.
Recurso: exportação/importação portátil, recuperação vazia e cópias externas opcionais.
Escrita mobile no checkout original permanece preservada e fora desta release.

## Próximos gates

CI no head da preparação; merge autorizado somente com quatro checks SUCCESS.
Workflow oficial release-windows.yml no mesmo head validado, sem duplicar execução.
Release não draft/prerelease com exatamente APK e SHA-256; confirmar tag, hash e assinatura
compatível com beta.15. Só então anunciar atualização disponível.

Ensaio físico em perfil descartável Windows/S23 pendente. Não apagar acervo real.
NH-087, NH-085 e Fase 5 continuam IN_PROGRESS. Guia: docs/backup/BACKUP_EXTERNO.md.