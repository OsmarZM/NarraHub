# Assistência à escrita

## Objetivo

Oferecer recursos de produtividade sem alterar silenciosamente o texto do escritor e sem transformar recursos opcionais em dependências do editor.

## Recursos locais

- O corretor ortográfico usa o dicionário disponibilizado pelo sistema/WebView e pode ser ligado ou desligado na barra do editor.
- O autocomplete sugere personagens do universo ativo e palavras recorrentes do escritor. `Tab` aceita a sugestão selecionada; setas percorrem as opções; `Esc` fecha a lista.
- O vocabulário recorrente fica em `narrahub.writerVocabulary` no armazenamento local, limitado às 300 entradas mais relevantes. O texto integral dos capítulos não é copiado para esse índice.
- Fotos de personagens são renderizadas como decorações visuais do ProseMirror. Elas não entram no HTML salvo e aparecem antes de toda menção nominal exata.
- A anotação por voz usa o mecanismo de reconhecimento de fala exposto pelo dispositivo. A transcrição permanece editável no painel e só entra no capítulo quando o usuário escolhe **Inserir no capítulo**.

## Inteligência artificial opcional

### Configuração

| Campo | Origem | Destino | Regra |
| --- | --- | --- | --- |
| Modo | Preferências do usuário | `narrahub.ai.settings` | `off`, `local` ou `custom` |
| Endpoint | Gerenciado pelo NarraHub no modo local; preferência no modo próprio | `narrahub.ai.settings` | Local usa `127.0.0.1:11439`; remoto exige HTTPS |
| Modelo | Perfil instalado com consentimento; preferência no modo próprio | Manifesto privado da instalação ou requisição remota | O escritor não informa nomes técnicos no modo local |
| Chave | Preferências do usuário | `sessionStorage` | Nunca é persistida no banco ou no `localStorage` |

Os modos local e API própria usam o contrato HTTP compatível com OpenAI em `POST /chat/completions`. A API própria não embute provedor ou segredo. A instalação local usa componentes oficiais fixados e verificados antes de ativá-los.

### Fluxo

1. O NarraHub mede CPU, núcleos, memória total/disponível, GPU, arquitetura, AVX2 e espaço livre e calcula um `AI Hardware Score`.
2. Ele recomenda `NarraAI Lite`, `NarraAI Standard` ou `NarraAI Advanced`, mas não inicia nenhum download automaticamente.
3. Na primeira utilização, o escritor escolhe um perfil e autoriza explicitamente a instalação. O progresso e o espaço necessário permanecem visíveis.
4. O instalador baixa o runtime CPU do `llama.cpp` e o GGUF correspondente por HTTPS, confere tamanho e SHA-256 e somente então grava o manifesto local.
5. Nas próximas aberturas, o motor inicia com o NarraHub em uma porta exclusiva. O modelo entra em repouso após cinco minutos sem uso para devolver memória ao sistema.
6. O escritor seleciona um trecho e usa o balão ancorado à seleção para corrigir, reescrever, detalhar, encurtar ou fornecer outra instrução, sem bloquear o restante da tela.
7. O NarraHub monta um contexto compacto com o universo ativo, cânone cadastrado, orientações do escritor, decisões aceitas e o trecho ou ficha em edição.
8. A resposta aparece no próprio balão e substitui a seleção ou entra no cursor somente após ação explícita.
9. O painel recolhível do capítulo também pode gerar e persistir um resumo editorial por IA.

### Fichas de entidades

- Tags são uma função de categorização universal e não criam campos.
- Campos editáveis pertencem somente a fichas de personagens, lugares, eventos, objetos e organizações e são persistidos em `entity_attributes`.
- A criação de entidade pode receber um briefing e pedir à IA nome, descrição e entre quatro e oito campos adequados ao tipo. A proposta permanece editável antes da criação.
- Uma ficha existente pode pedir novos campos sem repetir propriedades existentes ou gerar um resumo baseado apenas no cânone e nas relações cadastradas.
- Campos antigos criados como metadados de entidade são migrados para a ficha. Origem e destino de cena usam colunas próprias do capítulo.

### Contexto criativo local

- O escritor pode registrar orientações explícitas de estilo nas configurações.
- Ações aplicadas no editor e sugestões aceitas em fichas geram registros curtos, limitados e separados por universo.
- Esses registros ficam no `localStorage`, podem ser apagados pelo escritor e não treinam o modelo.
- O contexto é limitado às últimas decisões relevantes e a uma amostra compacta do cânone; o texto integral do universo não é enviado.

### Ciclo de vida local

- Runtime: `llama.cpp`, instalado na pasta privada de dados do aplicativo.
- Perfis: Lite para tarefas curtas, Standard como padrão e Advanced para análise narrativa mais pesada.
- Segurança inicial: execução em CPU, sem camadas de GPU, para não depender de drivers CUDA/Vulkan e reduzir falhas `0xc0000005`.
- Memória: `--sleep-idle-seconds 300` mantém o serviço disponível, mas permite que o modelo seja descarregado quando o escritor não usa IA.
- Diagnóstico: a saída do `llama-server` fica em `ai/logs/llama-server.log` na pasta de dados do NarraHub.
- Encerramento: processos iniciados pelo NarraHub são finalizados quando o aplicativo fecha.

### Componentes verificados

| Componente | Origem | Licença | Integridade |
| --- | --- | --- | --- |
| `llama.cpp` CPU x64 | [release oficial b10612](https://github.com/ggml-org/llama.cpp/releases/tag/b10612) | MIT | Arquivo e SHA-256 fixados no instalador |
| NarraAI Lite | [Qwen3 0.6B GGUF](https://huggingface.co/ggml-org/Qwen3-0.6B-GGUF) | Apache-2.0 | Tamanho e SHA-256 do artefato GGUF |
| NarraAI Standard | [Qwen3 1.7B GGUF](https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF) | Apache-2.0 | Tamanho e SHA-256 do artefato GGUF |
| NarraAI Advanced | [Qwen3 4B GGUF](https://huggingface.co/Qwen/Qwen3-4B-GGUF) | Apache-2.0 | Tamanho e SHA-256 do artefato GGUF |

### Comandos rápidos

- Digitar `/` no editor abre a paleta de geração junto ao cursor.
- Os comandos nativos incluem `/nome`, `/lugar`, `/personagem`, `/dialogo`, `/continuar` e `/sensorial`.
- O escritor pode criar e excluir seus próprios atalhos, definindo comando, nome e prompt. Eles ficam em `narrahub.promptShortcuts` no armazenamento local.
- A geração não é aplicada automaticamente: o resultado aparece no balão e exige a ação **Inserir aqui**.

## Exceções e limites

- Reconhecimento de fala depende do suporte e da permissão de microfone do sistema. O NarraHub não envia um arquivo gravado para um serviço próprio nem promete transcrição offline.
- O cliente HTTP nativo do Tauri evita bloqueios de CORS do WebView e permite diferenciar uma porta ocupada por outro programa de uma API de IA válida.
- A instalação gerenciada inicial está disponível no Windows 64 bits. Android e outros sistemas continuam exigindo uma implementação de runtime específica da plataforma.
- O download pode superar 2 GB no perfil Advanced. Espaço livre e memória mínima são validados antes da instalação.
- Respostas de IA podem conter erros. Nada é aplicado automaticamente e o escritor continua responsável por revisar o resultado.
- A V1 implementa o runtime gerenciado, contexto compacto e memória explícita de decisões aceitas. Perfil comportamental inferido, embeddings e AI Router permanecem como evolução separada; não há fine-tuning contínuo.
