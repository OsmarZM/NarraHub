# Handoff — publicação beta.14 Android

Usuário solicitou atualização publicada para testar no aparelho em 2026-10-02.
Branch `codex/release-0.10.0-beta.14`, a partir de `origin/mobile-shell` em `fe210d1`.
Bump coordenado dos manifests, locks, README, CHANGELOG e PROJECT_STATE, sem mudança funcional.
TASKS mantém NH-085 IN_PROGRESS. Release somente Android; seguir `docs/RELEASE_ANDROID.md`.

Antes de publicar: preflight local e CI 4/4 do PR no head final. Depois de merge autorizado,
disparar `release-windows.yml` em ref imutável, verificar execução SUCCESS, pré-release não
draft, dois assets esperados, SHA-256 baixado contra APK e assinatura com apksigner.
Registrar commit/tag/run e disponibilizar link; não inventar instalação ou PASS físico.
A qualificação física da beta.14 fica pendente conforme `docs/releases/0.10.0-beta.14.md`.
