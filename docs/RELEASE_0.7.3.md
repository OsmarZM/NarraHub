# NarraHub 0.7.3

Esta versão corrige falsos positivos na abertura do compartilhamento temporário. Em alguns casos, o `cloudflared` registrava uma conexão com a borda da Cloudflare e anunciava uma URL, mas o hostname não existia no DNS. O NarraHub mostrava a sessão como online mesmo que nenhum visitante conseguisse abrir o link.

## Correções

- A URL pública precisa responder a `/health` por HTTPS antes de aparecer como disponível.
- A resposta precisa identificar explicitamente o serviço `narrahub-share`; páginas de erro e respostas de outro serviço são rejeitadas.
- Hostnames sem DNS, conexões recusadas, timeouts e respostas inválidas geram uma nova tentativa automática.
- O aplicativo tenta até três Quick Tunnels antes de apresentar o diagnóstico ao escritor.
- Uma sessão anteriormente marcada como ativa é verificada novamente antes de criar outro link.
- A leitura da saída do sidecar aceita uma URL dividida entre blocos do processo.

## Segurança

A mudança não altera a criptografia. O conteúdo continua cifrado com AES-256-GCM no dispositivo, e a chave permanece somente no fragmento `#k=` da URL. A verificação pública consulta apenas o endpoint fixo `/health`, que não contém histórias, capítulos, entidades ou chaves.

## Validação

- A sessão problemática foi confirmada com servidor local saudável e conexão Cloudflare registrada, mas hostname público inexistente no DNS.
- Um Quick Tunnel isolado posterior resolveu no DNS e respondeu `200` ao `/health`, confirmando a natureza transitória do defeito externo.
- O backend passou a aceitar somente status HTTPS de sucesso com `ok: true` e `service: narrahub-share`.
- 11 testes Rust, 4 testes de contrato do compartilhamento e o bundle Angular de produção foram aprovados.

## Atualização

Atualize para a versão 0.7.3, feche qualquer sessão antiga e gere um novo link. Links de Quick Tunnels anteriores não são reaproveitados entre reinicializações.
