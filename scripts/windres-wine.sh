#!/usr/bin/env bash
# Invoke MinGW windres.exe under Wine for Linux-hosted Windows GNU builds.
set -euo pipefail
WINDRES_EXE="${OL_WINDRES_EXE:-$HOME/.local/mingw/mingw64/bin/windres.exe}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-ondalivre}"
export PATH="${HOME}/.local/wine-wow/bin:${PATH}"
exec wine "$WINDRES_EXE" "$@"
