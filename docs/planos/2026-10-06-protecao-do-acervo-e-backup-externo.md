# Proteção do acervo e backup externo

Data: 2026-10-06
Status: aprovado pelo usuário; implementação na branch codex/backup-externo
Escopo: proteção contra exclusão da pasta de dados, reinstalação e falhas de armazenamento.

## Objetivo

Permitir recuperar universos, manuscritos e mídias a partir de uma cópia controlada pelo usuário,
mesmo que toda a pasta de dados do NarraHub tenha sido removida. Manter funcionamento offline,
compatibilidade com snapshots existentes e as regras de identidade do Sync V2.

## Diagnóstico confirmado no código

- `database/mod.rs` resolve o banco com `app_data_dir()` e `narrahub.db`.
- `database/backup.rs` grava snapshots em `app_data/backups`, incluindo banco, assets e manifesto com hashes.
- A exclusão da raiz de dados elimina banco, mídias, identidade local e snapshots internos.
- A porta Angular `BackupService` oferece criar, listar, validar e restaurar snapshots locais; não oferece exportar/importar um arquivo externo.
- `recovery.rs::prepare_restore_at` exige um snapshot interno e cria um backup do banco ativo. A recuperação numa instalação sem banco precisa de um caminho explícito.
- A identidade privada `sync-identity.json` fica fora do backup existente e deve continuar fora do pacote portátil.

AppData é armazenamento persistente de aplicativo. Alterar o nome da pasta ou mudar o banco
para outro diretório não protege contra exclusão manual, falha de disco ou perda do aparelho.
O risco principal é a cópia de segurança compartilhar a mesma raiz de dados da cópia ativa.

Base arquitetural: ADRs 0001, 0006, 0008, 0009 e 0010. Esta proposta amplia a infraestrutura
de backup existente. Migração do diretório canônico e serviço próprio de nuvem ficam fora do escopo.

## Entregas e ordem

| Entrega | Resultado esperado | Gate de aceitação |
| --- | --- | --- |
| 1. Pacote portátil e recuperação | Exportar/importar banco, assets e manifesto versionado; restaurar também numa instalação vazia | Um perfil novo recupera o conteúdo de um pacote externo, sem depender da pasta original |
| 2. Interface Windows e Android | Escolher destino/origem pelo seletor nativo e mostrar proteção externa confirmada | Arquivo final aberto e validado após salvar; cancelamento e falha não produzem sucesso |
| 3. Cópias externas periódicas | Destino configurado, cópias por mudanças com intervalo limitado e retenção segura | Última cópia válida permanece disponível se a seguinte falhar ou o destino ficar indisponível |
| 4. Qualificação e publicação | Roteiro de recuperação, CI, beta e evidência real de Windows/S23 | Recuperação completa e conferência de conteúdo/mídia em perfil isolado |

A primeira entrega pública precisa incluir exportação e importação funcional. Os PRs podem
separar núcleo e interface, mas o recurso só é considerado pronto após o ciclo completo.

## Entrega 1 — formato e recuperação

1. Definir um arquivo `.narrahub-backup`, inicialmente um contêiner ZIP com formato identificado
   e versionado; manter o manifesto interno compatível com os snapshots atuais.
2. Criar o snapshot com o mecanismo consistente do SQLite já utilizado, após confirmar o
   salvamento das alterações atuais. Empacotar o snapshot validado, sem copiar o banco ativo diretamente.
3. Incluir somente `narrahub.db`, assets previstos no manifesto e metadados necessários.
   Não copiar a raiz inteira do aplicativo, arquivos de identidade privada, cache ou modelos de IA.
4. Auditar credenciais e preferências presentes no banco antes de fixar o contrato portátil.
   Não presumir que excluir arquivos de identidade elimina todos os segredos do banco.
5. Ao importar, extrair para staging controlado, verificar versão, hashes, compatibilidade de
   schema, integridade SQLite e foreign keys. Recusar caminhos absolutos, traversal, links,
   arquivos duplicados, entradas não permitidas e expansão excessiva do pacote.
6. Se existir acervo ativo, confirmar substituição, criar snapshot de segurança e executar a
   troca pelo fluxo de rollback existente; encerrar escrita, sync e compartilhamento conforme o contrato atual.
7. Se a instalação estiver vazia, permitir selecionar/importar antes de criar um acervo novo.
   Validar tudo antes de publicar o banco e assets; uma falha mantém a instalação recuperável.
8. Reusar o bootstrap de identidade/época existente: restauração preserva a autoria registrada
   no acervo e respeita as regras já qualificadas para identidade local e confiança entre aparelhos.

Dados narrativos são sensíveis. Um pacote sem criptografia deve ser identificado claramente
como legível por quem acessar o arquivo. Criptografia por senha pode ser uma entrega própria,
com formato, biblioteca e recuperação da senha definidos antes da implementação. Não criar
criptografia própria nem gravar a senha no pacote ou em logs.

## Entrega 2 — experiência e destinos

Em Configurações → Backup:

- “Criar cópia neste dispositivo”: recuperação de migration/update/edição indevida.
- “Salvar backup em outro local”: escolhe um destino fora da raiz do aplicativo.
- “Restaurar de arquivo”: escolhe um pacote, mostra data, versão e resumo antes da confirmação.
- Exibir a data da última exportação confirmada e comunicar que snapshots internos desaparecem
  com a exclusão dos dados do aplicativo.

Windows: sugerir uma pasta identificável escolhida pelo usuário, por exemplo Documentos/NarraHub Backups;
para falha do disco, recomendar uma segunda unidade ou mídia externa. Recusar destino dentro da
raiz do aplicativo e locais temporários conhecidos. Confirmar o destino final por leitura/validação.

Android: usar o seletor de documentos do sistema para criar/abrir o pacote fora do armazenamento
privado do app. Verificar capacidades reais dos plugins; se necessário, implementar um adaptador
nativo mínimo. Não tratar URI de documento como caminho de filesystem comum nem exigir acesso
amplo ao armazenamento. Um provedor escolhido pelo usuário pode ter comportamento diferente
de um arquivo local; a evidência de conclusão deve refletir o que a plataforma permite verificar.

UI → `core/native/backup.service.ts` → comandos de plataforma → infraestrutura Rust.
Operações de arquivo e dados não entram diretamente nos componentes Angular.

## Entrega 3 — rotina externa

- Configuração explícita do destino e habilitação de cópia periódica pelo usuário.
- Criar por mudanças com intervalo limitado; não gerar pacote a cada tecla nem depender apenas
  do fechamento do app, que pode ser encerrado pelo sistema.
- Windows: lidar com unidade desconectada, permissão revogada e falta de espaço.
- Android: verificar suporte a permissão persistente do destino e a possibilidade de reabertura;
  não prometer execução periódica em segundo plano sem implementação e qualificação nativas.
- Validar o pacote novo antes de aplicar retenção. Remover somente arquivos gerenciados pelo
  NarraHub nesse destino; preservar exportações manuais e arquivos alheios.
- Falha de backup externo não impede a escrita local; deixa um estado de atenção claro.
- Configurações perdidas com AppData não impedem restaurar: o seletor de arquivo funciona sem
  depender do destino previamente configurado.

## Verificação obrigatória

- Exportação com WAL ativo e conteúdo salvo, mídias completas e manifesto consistente.
- Importação de snapshot antigo suportado e recusa de schema mais novo.
- Pacote truncado/corrompido, arquivo sem mídia, ZIP malicioso e falta de espaço.
- Cancelamento do seletor, permissão negada e destino desconectado sem falso sucesso.
- Recuperação numa instalação vazia e substituição de acervo existente com rollback.
- Identidade privada ausente do pacote; comportamento de confiança/época preservado.
- Interrupção durante exportação/troca; pacote anterior e dados ativos preservados.
- Windows real e Galaxy S23 real: salvar, importar, abrir capítulos/imagens e reiniciar.
- Gates do repositório e CI completo antes de merge/publicação.

O ensaio de exclusão usa exclusivamente um perfil de qualificação descartável e uma cópia do
acervo. Nunca apagar a pasta real do usuário para testar. Relatar testes automatizados e
validação física separadamente.

## Próximos passos práticos

1. Preservar as alterações da Escrita mobile e fechar sua entrega na branch atual conforme os
   gates já definidos. A proteção do acervo terá branch e PR próprios.
2. Priorizar exportação/importação manual antes de avançar para novas telas da Fase 5.
3. Fixar o contrato do pacote, limites de tamanho, política de credenciais e comportamento
   de recuperação vazia; identificar os adaptadores Windows/Android necessários.
4. Implementar núcleo e interface em fatias pequenas, executar o ciclo completo de recuperação
   em perfil isolado e publicar uma beta para o ensaio físico.
5. Só então implementar periodicidade externa e eventual criptografia por senha.

## Proteção provisória no Windows

Enquanto exportação externa não existe: criar um backup pelo app, aguardar confirmação de
conclusão e copiar o diretório completo desse snapshot de `AppData/.../backups` para uma pasta
externa. Copiar banco, assets e manifesto juntos; não copiar apenas `narrahub.db` ativo nem
diretórios `.tmp-*`. Essa cópia exige, hoje, procedimento técnico para voltar à área de backups
do aplicativo e usar o restore existente; o importador da entrega 1 elimina essa dependência.

No Android, a cópia manual da pasta privada não é um fluxo acessível normal. Até haver exportação
externa e recuperação qualificadas, preservar os dados do aplicativo durante atualizações.
