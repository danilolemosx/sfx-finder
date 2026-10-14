# Como trabalhar neste projeto

Modelo de app desktop do Danilo (editor de vídeo, Avid Media Composer), baseado no Decanta
(github.com/danilolemosx/tratamento-audio-app). O processo completo está em `docs/processo.md`. Leia antes de começar.

## Conversa
- Responder sempre em **português**, de forma **concisa**. O Danilo não é programador: explicar o que muda para ele, não o código.
- Trabalhar em **etapas com checkpoint**: ao fim de cada etapa, dizer o que ficou pronto e o que vem a seguir.
- **Antes de programar a interface, mostrar um preview** (imagem renderizada de um HTML de mockup) e iterar até ele aprovar.
  Mostrar a estética e também a operação: ele quer as duas coisas juntas, tudo numa tela só sempre que possível.
- Quando ele pedir "não faça ainda, apenas responda", só responder.
- Dar estimativa de tempo quando pedirem e oferecer o caminho mais simples quando houver trade-off.

## Código
- Toda a lógica no crate `motor/` (testável sozinho, com `motor-cli`); `app/` só liga a interface ao motor.
- Interface em HTML/CSS/JS puros em `ui/`, textos em português, fonte Archivo (`ui/fonts/`).
- Commits pequenos e descritivos, em português. Rodar `cargo test -p motor` antes de cada push.
- Se o app tiver de ser idêntico a um processo de referência, criar um teste automático que compare bit a bit (ver `docs/processo.md`).
- Testar o app de verdade antes de entregar: `xvfb-run` + captura de tela (Linux), e ver o resultado.

## Renomear o modelo para um app novo
Trocar "Modelo"/"modelo" em: `app/tauri.conf.json` (productName, mainBinaryName, identifier, title),
`app/Cargo.toml` (name, lib name), `app/src/main.rs` (modelo_lib), `.github/workflows/*.yml`, `ui/index.html`,
`README.md`. Gerar ícones: `cd app && cargo tauri icon icons/icon-fonte.png -o icons` (apagar `android/` e `ios/`).
