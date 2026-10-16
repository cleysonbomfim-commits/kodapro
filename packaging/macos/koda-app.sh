#!/usr/bin/env bash
# Koda Pro: monta "Koda Pro.app" para uso local no Mac (abre com dois cliques, nome no Dock e na
# barra de menus). Para releases assinadas e notarizadas use package.sh.
#
# Uso: packaging/macos/koda-app.sh [--skip-build] [--install]
#   --skip-build  usa o target/release/filmcraft já compilado
#   --install     copia o app para ~/Applications
# Saída: dist/Koda Pro.app
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BUILD=1
INSTALL=0
for a in "$@"; do
  case "$a" in
    --skip-build) BUILD=0 ;;
    --install) INSTALL=1 ;;
    *) echo "uso: $0 [--skip-build] [--install]" >&2; exit 2 ;;
  esac
done

NAME="Koda Pro"
BUNDLE_ID="io.github.cleysonbomfim-commits.kodapro"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
SHORT_VERSION="${VERSION%%-*}"
SHA="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
APP="$ROOT/dist/$NAME.app"

if [ "$BUILD" = 1 ]; then
  (cd "$ROOT" && cargo build --release -p filmcraft)
fi
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/filmcraft"
[ -x "$BIN" ] || { echo "erro: $BIN não existe; rode sem --skip-build" >&2; exit 1; }

echo "==> montando $APP ($VERSION, $SHA)"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/$NAME"
cp "$ROOT/assets/app-icon/filmcraft.icns" "$APP/Contents/Resources/$NAME.icns"
# Mesmo Info.plist do FilmCraft, com o nome, o identificador e os textos do Koda Pro. O tipo de
# documento .fcproj continua o mesmo, para abrir os projetos existentes.
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" -e "s/@BUILD_SHA@/$SHA/g" \
  -e "s|<string>FilmCraft</string>|<string>$NAME</string>|g" \
  -e "s|<string>ai.storyteller.filmcraft</string>|<string>$BUNDLE_ID</string>|" \
  -e "s|FilmCraft uses the microphone|$NAME uses the microphone|" \
  -e "s|Copyright © the FilmCraft authors.|Copyright © the FilmCraft and $NAME authors.|" \
  "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"

# Assinatura ad hoc (só para este Mac); o microfone continua liberado pelos entitlements.
codesign --force --sign - --entitlements "$HERE/entitlements.plist" "$APP/Contents/MacOS/$NAME"
codesign --force --sign - --entitlements "$HERE/entitlements.plist" "$APP"
codesign --verify --strict --verbose=1 "$APP"

if [ "$INSTALL" = 1 ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/$NAME.app"
  ditto "$APP" "$HOME/Applications/$NAME.app"
  echo "==> instalado em ~/Applications/$NAME.app"
fi
echo "==> pronto: $APP"
