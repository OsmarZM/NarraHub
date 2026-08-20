# Sincronização local

## Fluxo

1. No dispositivo receptor, abra Configurações e escolha **Disponibilizar na rede**.
2. O NarraHub mostra endereço IP, porta e código temporário.
3. No outro dispositivo, informe endereço e código.
4. O iniciador envia seu snapshot local autenticado pelo código.
5. O receptor faz merge e devolve o estado consolidado.
6. O iniciador aplica o retorno e atualiza a interface.

## Merge

- Tabelas são processadas em ordem de dependência.
- Registros inexistentes são inseridos.
- Registros com `updated_at` só são atualizados quando o remoto é mais recente.
- Capítulos com o mesmo `updated_at` e conteúdos diferentes não são sobrescritos; o conflito é armazenado.
- A operação inteira usa uma transação SQLite.

## Segurança

O código de pareamento é aleatório, possui seis dígitos e existe apenas enquanto o receptor está disponível. Ele impede conexão acidental, mas não substitui criptografia.

Não use a versão 0.2 em redes públicas ou não confiáveis. A próxima versão do protocolo deve incluir:

- identidade persistente por chave pública;
- TLS com pinning ou protocolo Noise;
- descoberta DNS-SD/mDNS;
- tombstones para exclusões;
- sincronização incremental por cursor;
- transferência de anexos por hash e blocos;
- interface de resolução de conflitos.
