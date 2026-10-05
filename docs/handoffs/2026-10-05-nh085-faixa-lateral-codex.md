# NH-085 — Correção da faixa fixa à direita

## Causa e correção

Em 2026-10-05 o usuário relatou uma faixa fixa à direita na beta.14. A margem global de48px
na `.nh-mshell-content`, introduzida na fatia1 para afastar ações da alça, reduzia toda a área
útil da página e expunha uma coluna do fundo do shell. Reprodução no navegador: viewport412,
conteúdo terminando em364, diferença48. O gate geométrico falhou antes da alteração.

Removida somente essa margem. Conteúdo volta a preencher a largura disponível; a alça continua
sobreposta localmente, 48×132px e centro em60%, com os mesmos gestos e contrato nativo.
Não há mudança de domínio, Sync V2, schema, persistência ou navegação desktop.
A estratégia anterior de reservar uma coluna inteira fica superada por esta correção.

## Validação

CLI Playwright em Chromium com toque: Biblioteca e Configurações × claro/escuro ×
375/390/412/430px =16 combinações PASS, diferença lateral0, sem rolagem horizontal.
Toque na alça abriu o relógio. Capturas antes/depois inspecionadas visualmente.
A execução inicial de fechamento pelo script usou um método inexistente e depois foco
inadequado para Escape; o roteiro final usa somente abertura real por toque. Fechamento e
arrasto continuam cobertos pela suíte existente mobile-shell, executada separadamente.

Preflight completo e suíte mobile-shell iniciados nesta sessão; resultados finais devem ser
conferidos antes do merge/publicação. Logs locais: output/beta15-preflight.log,
output/beta15-mobile-shell.log, output/faixa-geometria-final.log.

## Entrega

Branch `codex/nh-085-corrige-faixa-lateral` a partir do merge PR80 `e8b892c`.
Beta.15 preparada para distribuir a correção, usando o workflow oficial Android assinado,
com todos os checks do PR aprovados antes do merge/disparo. Ver docs/RELEASE_ANDROID.md.
NH-085 permanece IN_PROGRESS; teste desta correção no S23 e M15 físico pendentes.
