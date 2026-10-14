# SFX FINDER — busca e organiza bancos de efeitos sonoros

App para Mac (Apple Silicon e Intel) e Windows que acha os sons do seu banco de SFX e leva os escolhidos para
uma pasta temática, copiados ou convertidos.

- **Primeira vez**: escolha a pasta do banco. O app lê tudo uma vez e guarda um índice (62 mil sons ≈ 12 min num HD);
  depois abre na hora. "Atualizar" lê só o que mudou.
- **Busca** em português ou inglês ("porta batendo" também acha "door slam"), com `-palavra` para excluir,
  filtros de duração, qualidade, canais e gosto, e navegação por biblioteca.
- **Ouvir**: espaço toca, ↑↓ troca de som, arrastar na forma de onda marca um trecho.
- **Gosto**: 👍 sobe o som nas buscas, 👎 desce; o app marca os sons já usados e para onde foram.
- **Cesta**: enter põe o som na cesta; nome traduzido e/ou encurtado (editável); **copiar** o original ou
  **converter** nos formatos do Destila (WAV, MXF Avid, AIFF, MP3…), com ORIGEM.txt.
- **Arrastar** um som da lista direto para o Finder ou para um bin do Avid.

Por dentro: [Tauri v2](https://tauri.app), motor em Rust (`motor/`: índice SQLite com busca FTS5, dicionário
português ↔ inglês em `motor/src/dicionario/`), e programas embutidos compilados no GitHub Actions:
**ffmpeg/ffprobe** (LGPL) e **raw2bmx** (BBC bmx, para o MXF).

## Desenvolvimento

```bash
cargo test -p motor                                   # testes (precisam de ffmpeg no PATH; MXF precisa de raw2bmx)
cargo run -p motor --bin motor-cli -- indexar PASTA   # lê o banco (mesmo índice do app)
cargo run -p motor --bin motor-cli -- buscar porta batendo --max 5
cargo run -p motor --bin motor-cli -- nome "Door Metal Slam 02" -t -e
scripts/compilar-ferramentas.sh aarch64-apple-darwin  # compila ffmpeg/ffprobe/raw2bmx em app/binaries
testes-ui/capturar.sh                                 # captura a interface com dados simulados
cd app && npx @tauri-apps/cli@2 dev                   # abre o app
```

## Publicar uma versão

1. Atualize `"version"` em `app/tauri.conf.json`.
2. Confira na aba **Actions** que **Testes** e **Ferramentas embutidas** passaram.
3. Em **Releases → Draft a new release**, crie a tag `vX.Y.Z`, dê o título `SFX Finder vX.Y.Z` e clique em
   **Publish release**.
4. Em 20 a 30 minutos, os instaladores aparecem no fim da página da Release:
   `SFX Finder_X.Y.Z_aarch64.dmg` (Mac M1–M4), `SFX Finder_X.Y.Z_x64.dmg` (Mac Intel) e
   `SFX Finder_X.Y.Z_x64-setup.exe` (Windows).

## Licenças
- SFX Finder: MIT OR Apache-2.0.
- FFmpeg (LGPL 2.1+), LAME (LGPL), Opus e Vorbis (BSD), bmx (BSD 3-Clause), embutidos como programas separados.
  Os códigos-fonte e as versões exatas estão em `scripts/compilar-ferramentas.sh`.
- Fonte Archivo (Omnibus-Type): SIL Open Font License 1.1, veja `ui/fonts/OFL.txt`.
