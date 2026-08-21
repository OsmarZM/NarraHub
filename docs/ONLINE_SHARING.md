# Compartilhamento temporário

## Objetivo

Permitir que o autor compartilhe uma seleção somente para leitura enquanto o computador e o NarraHub estiverem ativos, sem conta, domínio ou hospedagem permanente.

## Funcionamento

1. O autor clica em **Compartilhar** dentro de um universo.
2. Seleciona a apresentação do universo, capítulos e fichas de personagens, lugares, eventos, objetos ou organizações.
3. O aplicativo reduz imagens grandes e serializa somente os itens marcados.
4. Uma chave aleatória AES-256-GCM e um IV de 96 bits são criados no dispositivo.
5. O pacote cifrado fica apenas na memória do processo Rust.
6. O sidecar `cloudflared` abre um Quick Tunnel HTTPS aleatório `*.trycloudflare.com` para o servidor local embutido.
7. A chave é adicionada ao fragmento `#k=` da URL. Esse fragmento não é enviado em requisições HTTP.
8. O navegador do visitante busca o envelope e descriptografa localmente.
9. Ao clicar em **Encerrar** ou fechar o NarraHub, o processo do túnel é finalizado e todos os envelopes em memória são descartados.

## Limites e segurança

- Qualquer pessoa com a URL completa consegue ler os itens selecionados.
- A Cloudflare pode observar metadados de transporte, como IP, horário e volume, mas não recebe a chave de leitura.
- O pacote aberto nunca é enviado ao túnel; a criptografia ocorre antes da publicação.
- O servidor local aceita somente leitura pública e mantém os envelopes em RAM.
- O limite por pacote cifrado é 2,8 MB; o cliente limita o texto aberto a 2 MB e reduz imagens grandes.
- A validade de 1, 7 ou 30 dias é um teto. A sessão termina antes ao fechar o aplicativo.
- Não há edição colaborativa, conta, descoberta pública, indexação ou persistência online.
- O compartilhamento é diferente da sincronização Wi-Fi e não altera o banco local do autor.

## Distribuição Windows

O workflow de release baixa uma versão fixa do `cloudflared-windows-amd64.exe`, valida o SHA-256 publicado e o empacota como sidecar Tauri. O executável não precisa estar instalado separadamente no computador do usuário.

O `externalBin` fica em `src-tauri/tauri.windows.conf.json`, portanto não interfere no build Android.

## Servidor de desenvolvimento legado

`services/share-api` permanece como implementação compatível para testes automatizados do visualizador e para quem optar por uma hospedagem persistente no futuro. Ele não é usado pelo fluxo padrão do aplicativo Windows.
