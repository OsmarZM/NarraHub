# Compatibilidade entre aplicativo e banco

| Linha do aplicativo | Schema máximo | Situação |
|---|---:|---|
| `0.7.4` publicada | 10 | Release pública e assinada |
| próxima versão em desenvolvimento | 13 | Planejamento com cards tipados, relações normalizadas e migração de dados legados; ainda não publicada |

## Regra

Migrations publicadas ou já aplicadas são imutáveis. As migrations 11, 12 e 13 são novas e não modificam os SQLs das versões 1 a 10. Um banco atualizado para schema 13 não deve ser aberto por executável antigo como estratégia de rollback; recuperação usa o backup anterior à atualização.

Antes de publicar a versão que contém schema 13, são obrigatórios:

1. backup válido de um banco schema 10;
2. upgrade 10 → 11 → 12 → 13 no runtime Tauri;
3. reabertura depois de reiniciar;
4. validação de capítulos, entidades, timeline, tags e planejamento;
5. restauração testada do snapshot anterior;
6. instalador, assinatura, `latest.json` e detecção pelo updater verificados separadamente.
