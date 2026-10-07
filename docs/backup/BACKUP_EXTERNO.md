# Backup externo e recuperação do acervo

## Objetivo

Recuperar universos, manuscritos e mídias mesmo após perder a pasta privada do NarraHub.
AppData é armazenamento persistente; não deve ser apagado como se fosse cache. Snapshots
internos compartilham a raiz do acervo e desaparecem se essa raiz for excluída.

## Uso

Em **Configurações → Geral → Backup e integridade**:

1. **Salvar backup em outro local** abre o seletor do sistema. Escolha uma pasta identificável,
   fora dos dados do app. Para proteger contra perda do disco, use outra unidade ou mídia.
2. A confirmação aparece depois de reler o arquivo final e comparar tamanho e SHA-256.
3. **Restaurar de arquivo** importa e valida o pacote, sem substituir o acervo ativo.
4. Confira o resumo, prepare a restauração e digite `RESTAURAR`. O estado atual ganha snapshot
   de segurança, e a troca preserva o rollback existente.

Em um perfil sem banco, o arranque oferece **Abrir backup externo** e **Criar novo acervo**
antes de abrir o pool SQLite. A recuperação não depende da configuração de destino perdida.
Nesse caso não há banco anterior para salvar: `safetyBackupId` é uma string vazia.

O arquivo contém conteúdo narrativo **sem criptografia**. Proteja o acesso ao destino. Ele
não transporta a chave privada do aparelho, modelos de IA nem preferências de Web Storage.
Sessões de compartilhamento ficam encerradas na cópia portátil, com chaves/tokens removidos;
o histórico de propostas é preservado. O acervo ativo não é sanitizado.

## Formato e validação

Extensão: `.narrahub-backup`. Contêiner ZIP, identificado pelo comentário
`narrahub-backup-v1`, contendo exatamente:

- `manifest.json`: manifesto v1 usado pelos snapshots internos;
- `narrahub.db`: snapshot consistente, produzido pela API de backup SQLite;
- `assets/<path>`: somente arquivos declarados no manifesto.

Limites: 8 GiB para o arquivo de entrada e para o conteúdo expandido declarado, 50.000 entradas,
16 MiB para o manifesto. Nenhuma entrada é extraída antes da conferência da lista de arquivos,
nomes, tamanhos, links e duplicatas. Caminhos absolutos, traversal, ADS e aliases de dispositivos
Windows são recusados. Depois da extração são conferidos hashes, tamanhos, schema,
`integrity_check` e `foreign_key_check`. Schema mais novo exige atualizar o app.

A importação publica um snapshot manual com identificador novo. Ela verifica primeiro o pacote
original, sanitiza credenciais somente na cópia importada e recalcula/valida o manifesto.
Uma importação inválida não toca no banco ativo. A restauração confere novamente banco e mídias
preparados antes da troca. Snapshots internos antigos continuam usando o fluxo existente.

## Cópias periódicas

São desativadas por padrão. **Escolher pasta e ativar** solicita um destino e tenta a primeira
cópia. O monitor consulta a cada minuto enquanto o app está aberto e visível; o Rust limita
tentativas a intervalos de pelo menos 30 minutos e compara mudanças do banco/WAL. **Copiar agora**
permite uma tentativa explícita. O monitor espera o autosave concluir e não inicia uma escrita
concorrente com a digitação. Restauração preparada, update e outra operação de backup suspendem
a tentativa do frontend; o backend também possui exclusão mútua.

As cinco cópias automáticas mais recentes deste perfil são mantidas. A retenção ocorre depois
da conferência da cópia nova e só alcança arquivos regulares com o namespace e formato gerados
pelo perfil. Exportações manuais, pastas e arquivos alheios são preservados. Falha mantém a
última confirmação conhecida e mostra atenção; não impede a escrita local.

### Windows

O destino é resolvido e recusado se estiver na raiz de dados do aplicativo ou na pasta temporária
do processo. A gravação usa arquivo parcial vizinho, conferência e substituição recuperável.
Unidade desconectada e permissão/falta de espaço são erros de cópia, sem confirmação de sucesso.

### Android

Exportação/importação manual usam o seletor de documentos e `content://`, sem acesso amplo ao
armazenamento. A rotina usa `ACTION_OPEN_DOCUMENT_TREE` e permissão persistente de leitura/escrita.
O adaptador só lê o pacote temporário preparado na raiz privada do app. Grava, relê, confere e
renomeia o documento antes de confirmar. Provedores sem leitura, permissão persistente ou rename
podem ser recusados. A rotina não promete execução com o app em segundo plano ou fechado.

Após restore, o Windows reinicia o processo. No Android, o pool fechado e o estado nativo
`Unprepared` permitem recarregar a WebView e executar novamente o bootstrap protegido. O recurso
de restart de processo desktop não é usado para tentar executar o processo Java.

## Ensaio de recuperação

Use um perfil de qualificação descartável e uma cópia do acervo. Nunca apague a pasta real para
testar. Anote quantidade de universos/capítulos, texto de referência e mídias antes do ensaio.

1. Salve um texto e uma imagem; exporte para fora do perfil e aguarde confirmação.
2. Abra o arquivo em outro perfil vazio, confira o resumo e restaure.
3. Confira texto, imagens e quantidades; reabra o app e repita a conferência.
4. Em perfil com dados, importe outro pacote, cancele e confirme que nada mudou.
5. Restaure com confirmação; confira o snapshot anterior e o rollback.
6. Recuse arquivo truncado, mídia adulterada e schema futuro sem mudar o acervo.
7. Configure cópias externas; teste destino indisponível/revogado e preservação da cópia anterior.

Windows real e S23 real são gates separados de testes Rust, browser e CI. Nenhuma evidência
automatizada deve ser registrada como PASS físico.
