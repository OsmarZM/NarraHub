# NH-087 — proteção do acervo e backup externo

Data: 2026-10-07 · Owner: Codex · Branch: `codex/backup-externo` · Base: `mobile-shell`.
Status: implementação em qualificação; não DONE, não publicada.

## Autorização e preservação

O usuário aprovou a implementação do plano de proteção do acervo em 2026-10-06.
O checkout `narrahub-app`, na branch `codex/nh-085-escrita-editor`, mantém suas alterações locais
de Escrita e documentação. A proteção foi implementada no worktree `narrahub-backup-externo`.
Sync V2, migrations publicadas, identidade privada e diretório canônico não foram redesenhados.

## Implementação

- `database/portable.rs`: pacote com lista explícita, limites, staging, validação e sanitização
  de chaves/tokens de compartilhamento na cópia; preserve histórico e acervo ativo.
- `backup_external_commands.rs`: seletores, leitura final, destino externo, status, periodicidade
  por mudança e retenção limitada ao namespace do perfil.
- `BackupPlugin.kt`: SAF com árvore persistente, origem restrita, readback, rename e retenção.
- `recovery.rs`: instala também sem banco ativo; revalida mídias no commit antes da troca.
- `BackupService`/`SettingsStore`: fronteira nativa, flush manual, espera de autosave no monitor,
  exclusão mútua, status/erro e ações externas.
- Configurações: mantém temas, menu lateral e snapshots internos; adiciona exportação/importação
  e cópias externas opcionais, com aviso de conteúdo sem criptografia.
- Arranque vazio: escolha de recuperar ou criar antes do pool; loader impede montar rotas durante
  criação inicial. Recovery por schema também aceita arquivo externo.
- Android refaz o bootstrap pela WebView após restore; Windows usa restart do processo.
- Teste de contrato Android normaliza CRLF antes de localizar jobs YAML: no checkout Windows
  havia dois falsos negativos por `\r\n`, sem alteração no workflow de release.

## Evidência disponível na retomada

- Build Angular PASS em 2026-10-06.
- Arquitetura 105/105 PASS; IA 5/5; planning 4/4; share API 4/4 PASS.
- UI de produção, versão e configuração desktop PASS; contrato Android 7/7 PASS após ajuste CRLF.
- Primeiro pacote portátil: 6/6 PASS, incluindo WAL, mídias, ausência da identidade e sanitização.
- Browser: 23 PASS e 7 timeouts de startup/navegação; não registrar a suíte como PASS.
- Rust completo e build Android anteriores interrompidos sem veredito; retomados em 2026-10-07.

Logs locais ignorados pelo Git: `output/backup-architecture.log`, `backup-portable-test.log`,
`backup-rust-final.log`, `backup-e2e.log`, `backup-e2e-retest.log`, `backup-android-final.log`.
Resultados finais devem substituir a condição pendente somente após conclusão inequívoca.

## Gates restantes

1. Build final com as últimas mudanças, fmt/clippy e teste Rust completo.
2. Browser sem falhas; geometrias/temas em 375/390/412/430 e desktop; screenshots de viewport.
3. APK Android compilado, inclusive Kotlin e capabilities. Não basta `cargo check` desktop.
4. Quatro CI do PR no head final; merge/publicação somente depois dos gates completos.
5. Ensaio físico Windows/S23 em perfil descartável; publicação beta e assinatura têm evidências
   próprias. O acervo real não deve ser apagado. NH-087/NH-085/Fase 5 continuam IN_PROGRESS.

Contrato e roteiro: [BACKUP_EXTERNO.md](../backup/BACKUP_EXTERNO.md).
