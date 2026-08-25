# Compartilhamento e colaboração temporária

## Objetivo

Permitir que o autor compartilhe um ou mais universos enquanto o computador e o NarraHub estiverem ativos. Cada link define um escopo e uma permissão: somente leitura, anotações por seção ou propostas de edição.

Contribuições nunca alteram o conteúdo canônico diretamente. Elas são armazenadas na fila local de revisão e o autor aprova ou rejeita cada alteração, com opção de aprovação em lote.

## Escopo do link

O autor escolhe:

- um, vários ou todos os universos literários;
- inclusão de livros e capítulos;
- inclusão de fichas de personagens, lugares, eventos, objetos e demais entidades;
- validade máxima de 1, 7 ou 30 dias;
- permissão `view`, `comment` ou `edit`.

O visualizador web usa navegação semelhante ao aplicativo: seletor de universo, visão geral, biblioteca de capítulos e fichas abertas em modal. Isso evita páginas longas e fichas comprimidas.

## Permissões

| Permissão | Visitante | Entrada no banco local |
| --- | --- | --- |
| Somente leitura | Navega e lê o conteúdo selecionado | Nenhuma |
| Fazer anotações | Adiciona observações vinculadas a universo, capítulo ou ficha | Registrada como anotação da sessão |
| Propor edições | Envia novas versões de campos permitidos e também anotações | Registrada como pendente; só é aplicada após aprovação |

Campos editáveis são validados por lista permitida. Universos aceitam nome e descrição; capítulos aceitam título, resumo e texto; entidades aceitam nome, resumo, descrição, estado canônico e campos existentes da ficha.

## Fluxo técnico

1. O aplicativo monta um pacote `version: 3`, `kind: workspace` somente com o escopo selecionado.
2. Imagens grandes são reduzidas e o pacote aberto é limitado a 2 MB.
3. Uma chave AES-256-GCM e um IV de 96 bits são criados no dispositivo.
4. O pacote cifrado fica na memória do processo Rust.
5. O sidecar `cloudflared` abre um Quick Tunnel HTTPS aleatório `*.trycloudflare.com` para o servidor local embutido.
6. Antes de mostrar o endereço como online, o aplicativo acessa `/health` pela URL pública e exige a resposta identificada como `narrahub-share`.
7. Se o Quick Tunnel anunciar um hostname sem DNS ou sem HTTPS funcional, o processo é encerrado e repetido automaticamente até três vezes.
8. A chave fica no fragmento `#k=` da URL e não é enviada nas requisições HTTP.
9. O navegador descriptografa o workspace localmente.
10. Anotações e propostas são cifradas com a mesma chave e enviadas como envelopes opacos.
11. O aplicativo consulta novas contribuições a cada 2,5 segundos, descriptografa e persiste na fila SQLite.
12. Ao aprovar uma edição, o aplicativo valida alvo e campo, atualiza o registro local e grava o evento em `change_log`.

## Persistência e encerramento

As tabelas `collaboration_sessions` e `collaboration_contributions` preservam o histórico de revisão depois que o túnel é encerrado. Antes de revogar um link ou parar a sessão, o Angular busca as últimas contribuições ainda mantidas em memória.

Ao fechar o aplicativo, os links deixam de responder. Alterações já recebidas e persistidas continuam disponíveis em **Configurações > Compartilhar > Revisão local**.

## Segurança e limites

- Qualquer pessoa com a URL completa recebe a permissão definida para aquele link.
- A Cloudflare pode observar metadados de transporte, como IP, horário e volume, mas não recebe a chave do conteúdo.
- Conteúdo e contribuições trafegam cifrados; o servidor Rust não interpreta os textos.
- Um token de contribuição, incluído apenas dentro do pacote cifrado, é exigido para enviar anotações ou propostas.
- Cada link aceita no máximo 200 contribuições e 12 MB acumulados, reduzindo abuso de memória.
- Cada envelope cifrado aceita no máximo 2,8 MB e a requisição HTTP é limitada a 3 MB.
- O servidor aceita apenas campos revisáveis conhecidos; nomes de tabela e coluna não vêm do visitante.
- A sessão é colaborativa por revisão, não edição simultânea do mesmo documento ou CRDT.

## Distribuição Windows

O workflow de release baixa uma versão fixa do `cloudflared-windows-amd64.exe`, valida o SHA-256 publicado e o empacota como sidecar Tauri. O executável não precisa estar instalado separadamente.

O `externalBin` fica em `src-tauri/tauri.windows.conf.json`, portanto não interfere no build Android.

## Diagnóstico do túnel

O Quick Tunnel é um serviço temporário e pode, ocasionalmente, registrar a conexão com a borda da Cloudflare antes de o hostname público existir no DNS. Processo ativo ou mensagem de registro bem-sucedido não comprovam que o link funciona.

O NarraHub considera o túnel pronto somente quando todos estes pontos passam:

1. o servidor local responde em uma porta exclusiva da sessão;
2. o `cloudflared` publica uma URL HTTPS em `trycloudflare.com`;
3. a própria URL pública resolve e responde a `/health`;
4. o corpo da resposta confirma `service: narrahub-share`.

Se as três tentativas falharem, a interface permanece offline e mostra os motivos resumidos. Nesse caso, verifique resolução DNS, firewall, proxy corporativo e acesso HTTPS aos domínios `trycloudflare.com` e `argotunnel.com`.

## Servidor de desenvolvimento legado

`services/share-api` mantém o visualizador e os testes de contrato. O fluxo colaborativo padrão usa os endpoints em memória de `src-tauri/src/online_share.rs`; uma hospedagem persistente futura precisará implementar o mesmo contrato de contribuições cifradas.
