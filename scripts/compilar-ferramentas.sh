#!/bin/bash
# Compila os programas que vão embutidos no SFX Finder, sem dependências externas:
#   ffmpeg e ffprobe (LGPL, com lame, opus e vorbis) e raw2bmx (MXF do Avid).
# Saída: app/binaries/<programa>-<alvo>[.exe], como o Tauri espera em bundle.externalBin.
#
# Uso: scripts/compilar-ferramentas.sh <alvo> [ffmpeg|bmx|tudo]
#   alvos: aarch64-apple-darwin, x86_64-apple-darwin (no Mac),
#          x86_64-pc-windows-msvc (ffmpeg no MSYS2/MINGW64; bmx no Git Bash com Visual Studio),
#          x86_64-unknown-linux-gnu
set -euo pipefail

ALVO="${1:?informe o alvo, ex.: aarch64-apple-darwin}"
PARTE="${2:-tudo}"
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
TRAB="${SFXFINDER_TRAB:-$RAIZ/target/ferramentas/$ALVO}"
PREFIXO="$TRAB/prefixo"
SAIDA="$RAIZ/app/binaries"
mkdir -p "$TRAB/fontes" "$PREFIXO" "$SAIDA"

FFMPEG=7.1.1; LAME=3.100; OPUS=1.5.2; OGG=1.3.5; VORBIS=1.3.7; BMX=1.6
# nome do arquivo → "url sha256" (sem array associativo: o bash do Mac é o 3.2)
info_fonte() {
  case "$1" in
    ffmpeg-$FFMPEG.tar.xz) echo "https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz 733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1" ;;
    lame-$LAME.tar.gz) echo "https://downloads.sourceforge.net/project/lame/lame/$LAME/lame-$LAME.tar.gz ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e" ;;
    opus-$OPUS.tar.gz) echo "https://downloads.xiph.org/releases/opus/opus-$OPUS.tar.gz 65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1" ;;
    libogg-$OGG.tar.gz) echo "https://downloads.xiph.org/releases/ogg/libogg-$OGG.tar.gz 0eb4b4b9420a0f51db142ba3f9c64b333f826532dc0f48c6410ae51f4799b664" ;;
    libvorbis-$VORBIS.tar.xz) echo "https://downloads.xiph.org/releases/vorbis/libvorbis-$VORBIS.tar.xz b33cc4934322bcbf6efcbacf49e3ca01aadbea4114ec9589d1b1e9d20f72954b" ;;
    bmx-$BMX.tar.gz) echo "https://github.com/bbc/bmx/archive/refs/tags/v$BMX.tar.gz 41f163965cb7fffcc022ad4a099c295b3699709ce2306ee89b713cfcfda3b25a" ;;
  esac
}

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

# baixa (com conferência do sha256) e extrai numa pasta limpa; devolve o caminho
fonte() {
  local nome="$1" url sha arq pasta
  read -r url sha <<<"$(info_fonte "$nome")"
  arq="$TRAB/fontes/$nome"
  if [ ! -f "$arq" ] || [ "$(sha256 "$arq")" != "$sha" ]; then
    curl -fsSL --retry 3 -o "$arq" "$url"
  fi
  [ "$(sha256 "$arq")" = "$sha" ] || { echo "sha256 errado: $nome" >&2; exit 1; }
  pasta="$TRAB/src/${nome%.tar.*}"
  rm -rf "$pasta" && mkdir -p "$pasta"
  tar -xf "$arq" -C "$pasta" --strip-components=1
  echo "$pasta"
}

JOBS="$( (nproc || sysctl -n hw.ncpu || echo 4) 2>/dev/null | head -1)"
EXE=""
CFLAGS_ALVO="-O2"
HOST_ARGS=()
FF_ARGS=()
CMAKE_ARGS=()
case "$ALVO" in
  aarch64-apple-darwin)
    CFLAGS_ALVO="-O2 -arch arm64 -mmacosx-version-min=11.0"
    HOST_ARGS=(--host=aarch64-apple-darwin)
    FF_ARGS=(--arch=arm64 --target-os=darwin --enable-cross-compile)
    CMAKE_ARGS=(-DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0)
    ;;
  x86_64-apple-darwin)
    CFLAGS_ALVO="-O2 -arch x86_64 -mmacosx-version-min=10.15"
    HOST_ARGS=(--host=x86_64-apple-darwin)
    FF_ARGS=(--arch=x86_64 --target-os=darwin --enable-cross-compile)
    CMAKE_ARGS=(-DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=10.15)
    ;;
  x86_64-pc-windows-msvc)
    EXE=".exe"
    FF_ARGS=(--target-os=mingw32 --extra-ldflags=-static --pkg-config-flags=--static)
    CMAKE_ARGS=(-DBMX_SET_MSVC_RUNTIME=MT)
    ;;
  x86_64-unknown-linux-gnu)
    CMAKE_ARGS=()
    ;;
  *) echo "alvo desconhecido: $ALVO" >&2; exit 2 ;;
esac

compilar_ffmpeg() {
  export CFLAGS="$CFLAGS_ALVO" CXXFLAGS="$CFLAGS_ALVO" LDFLAGS="${CFLAGS_ALVO/-O2/}"
  export PKG_CONFIG_PATH="$PREFIXO/lib/pkgconfig"
  local comum=(--prefix="$PREFIXO" --enable-static --disable-shared ${HOST_ARGS[@]+"${HOST_ARGS[@]}"})
  local s

  s="$(fonte lame-$LAME.tar.gz)"
  sed -i.bak '/lame_init_old/d' "$s/include/libmp3lame.sym"   # símbolo que não existe mais (erro de link no Mac)
  (cd "$s" && ./configure "${comum[@]}" --disable-frontend --disable-decoder && make -j"$JOBS" && make install)

  s="$(fonte opus-$OPUS.tar.gz)"
  (cd "$s" && ./configure "${comum[@]}" --disable-doc --disable-extra-programs && make -j"$JOBS" && make install)

  s="$(fonte libogg-$OGG.tar.gz)"
  (cd "$s" && ./configure "${comum[@]}" && make -j"$JOBS" && make install)

  s="$(fonte libvorbis-$VORBIS.tar.xz)"
  sed -i.bak 's/-force_cpusubtype_ALL//g' "$s/configure"        # opção que o clang novo não aceita
  (cd "$s" && ./configure "${comum[@]}" --disable-examples --disable-docs --with-ogg="$PREFIXO" && make -j"$JOBS" && make install)

  s="$(fonte ffmpeg-$FFMPEG.tar.xz)"
  local encoders="pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_s16be,pcm_s24be,pcm_s32be,pcm_f32be,flac,alac,aac,ac3,eac3,mp2,wmav2,opus,vorbis,libmp3lame,libopus,libvorbis"
  local muxers="wav,w64,aiff,caf,flac,ipod,mp4,mov,mp3,mp2,ac3,eac3,ogg,opus,asf,null"
  (cd "$s" && ./configure --prefix="$PREFIXO" ${FF_ARGS[@]+"${FF_ARGS[@]}"} \
      --extra-cflags="$CFLAGS_ALVO -I$PREFIXO/include" --extra-ldflags="${CFLAGS_ALVO/-O2/} -L$PREFIXO/lib" \
      --pkg-config=pkg-config --pkg-config-flags=--static \
      --disable-shared --enable-static --disable-autodetect --enable-zlib \
      --disable-doc --disable-debug --disable-network --disable-ffplay --disable-devices --disable-hwaccels \
      --disable-encoders --enable-encoder="$encoders" --disable-muxers --enable-muxer="$muxers" \
      --enable-libmp3lame --enable-libopus --enable-libvorbis \
    && make -j"$JOBS")
  cp "$s/ffmpeg$EXE" "$SAIDA/ffmpeg-$ALVO$EXE"
  cp "$s/ffprobe$EXE" "$SAIDA/ffprobe-$ALVO$EXE"
  strip "$SAIDA/ffmpeg-$ALVO$EXE" "$SAIDA/ffprobe-$ALVO$EXE" 2>/dev/null || true
}

compilar_bmx() {
  local s b
  s="$(fonte bmx-$BMX.tar.gz)"
  b="$TRAB/build-bmx"
  rm -rf "$b"
  cmake -S "$s" -B "$b" -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DBMX_BUILD_TESTING=OFF -DBUILD_TESTING=OFF -DBMX_BUILD_TOOLS=OFF \
    -DBMX_BUILD_EXPAT_SOURCE=ON -DBMX_BUILD_URIPARSER_SOURCE=ON -DBMX_BUILD_WITH_LIBCURL=OFF \
    ${CMAKE_ARGS[@]+"${CMAKE_ARGS[@]}"}
  cmake --build "$b" --config Release --parallel "$JOBS" --target raw2bmx mxf2raw
  local exe
  exe="$(find "$b" -type f -name "raw2bmx$EXE" | head -1)"
  cp "$exe" "$SAIDA/raw2bmx-$ALVO$EXE"
  strip "$SAIDA/raw2bmx-$ALVO$EXE" 2>/dev/null || true
  # mxf2raw: só para os testes (não vai no instalador). Fora de target/, que o cache do Rust limpa.
  mkdir -p "$RAIZ/ferramentas-teste"
  cp "$(find "$b" -type f -name "mxf2raw$EXE" | head -1)" "$RAIZ/ferramentas-teste/mxf2raw-$ALVO$EXE"
}

case "$PARTE" in
  ffmpeg) compilar_ffmpeg ;;
  bmx) compilar_bmx ;;
  tudo) compilar_ffmpeg; compilar_bmx ;;
  *) echo "parte desconhecida: $PARTE" >&2; exit 2 ;;
esac
ls -la "$SAIDA"
