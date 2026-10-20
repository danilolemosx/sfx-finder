#!/bin/bash
# Baixa o modelo de IA que vai dentro do instalador (Qwen3.5-2B, Apache 2.0, quantizado em Q6_K pela Unsloth)
# para app/modelo/qwen.gguf, conferindo o sha256. Uso: scripts/baixar-modelo.sh
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
REV=f6d5376be1edb4d416d56da11e5397a961aca8ae
URL="https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/$REV/Qwen3.5-2B-Q6_K.gguf"
SHA=fc90339420b4298887aafb307a4291c55440b730133bbffe6ba9630503dcb548
ARQ="$RAIZ/app/modelo/qwen.gguf"
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
mkdir -p "$(dirname "$ARQ")"
if [ -f "$ARQ" ] && [ "$(sha256 "$ARQ")" = "$SHA" ]; then echo "modelo já está em $ARQ"; exit 0; fi
curl -fL --retry 3 -o "$ARQ.parcial" "$URL"
[ "$(sha256 "$ARQ.parcial")" = "$SHA" ] || { echo "sha256 errado no modelo" >&2; exit 1; }
mv "$ARQ.parcial" "$ARQ"
echo "modelo em $ARQ"
