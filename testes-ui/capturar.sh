#!/bin/bash
# Captura a interface com dados simulados (Tauri falso), uma imagem por cena, e lista os erros de JavaScript.
# Uso: testes-ui/capturar.sh [pasta-de-saida]
set -euo pipefail
raiz="$(cd "$(dirname "$0")/.." && pwd)"
saida="${1:-$raiz/testes-ui/capturas}"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
trabalho="$(mktemp -d)"
cp -R "$raiz/ui/." "$trabalho/"
cp "$raiz/testes-ui/mock.js" "$trabalho/"
cargo run -q --manifest-path "$raiz/Cargo.toml" -p motor --bin motor-cli -- --json-formatos > "$trabalho/estado.json"
printf 'window.__ESTADO__ = %s;\n' "$(cat "$trabalho/estado.json")" > "$trabalho/estado.js"
sed -i.bak 's#<script src="app.js"></script>#<script src="estado.js"></script><script src="mock.js"></script><script src="app.js"></script>#' "$trabalho/index.html"
mkdir -p "$saida"
for cena in principal primeira lendo sem-destino vazio convertendo; do
  "$chrome" --headless=new --hide-scrollbars --force-device-scale-factor=2 --window-size=1280,800 \
    --virtual-time-budget=4000 --screenshot="$saida/$cena.png" "file://$trabalho/index.html?cena=$cena" >/dev/null 2>&1
  erros="$("$chrome" --headless=new --virtual-time-budget=4000 --dump-dom "file://$trabalho/index.html?cena=$cena" 2>/dev/null | grep -o 'data-erros="[^"]*"' | tr '\n' ' ' || true)"
  echo "$cena: ${erros:-sem erros}"
done
