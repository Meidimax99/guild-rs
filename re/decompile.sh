#!/bin/sh
# Reproduce the decompiled source of GildeGold_TL.exe outside the repository.
#
#   re/decompile.sh <path/to/GildeGold_TL.exe> [outdir]   (default outdir: ../re-out)
#
# Imports the executable into a headless Ghidra project (first run only, ~2 min,
# followed by a search for functions auto-analysis missed), applies
# re/names.tsv, exports decomp.c / functions.tsv / strings.tsv and splits the
# decompile into per-module files under <outdir>/src/. If server/server.dll
# exists next to the executable it is decompiled into <outdir>/server/.
# Requires Ghidra (analyzeHeadless; set GHIDRA_HOME if not in /opt/ghidra).
set -eu
REPO=$(cd "$(dirname "$0")/.." && pwd)
EXE=${1:?usage: re/decompile.sh <GildeGold_TL.exe> [outdir]}
OUT=${2:-$REPO/../re-out}
HEADLESS=${GHIDRA_HOME:-/opt/ghidra}/support/analyzeHeadless
S=$REPO/re/ghidra_scripts
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
if [ ! -d "$OUT/proj.rep" ]; then
  "$HEADLESS" "$OUT" proj -import "$EXE" -scriptPath "$S" -postScript FindFunctions.java > "$OUT/import.log" 2>&1
fi
"$HEADLESS" "$OUT" proj -process "$(basename "$EXE")" -noanalysis -scriptPath "$S" \
  -postScript ApplyNames.java "$REPO/re/names.tsv" \
  -postScript ExportAll.java "$OUT" > "$OUT/export.log" 2>&1
python3 -I "$REPO/re/scripts/split_decomp.py" "$OUT/decomp.c" "$OUT/src"
SRV="$(dirname "$EXE")/server/server.dll"
if [ -f "$SRV" ]; then
  mkdir -p "$OUT/server"
  if [ ! -d "$OUT/server/proj.rep" ]; then
    "$HEADLESS" "$OUT/server" proj -import "$SRV" -scriptPath "$S" -postScript FindFunctions.java > "$OUT/server/import.log" 2>&1
  fi
  "$HEADLESS" "$OUT/server" proj -process server.dll -noanalysis -scriptPath "$S" \
    -postScript ApplyNames.java "$REPO/re/names_server.tsv" \
    -postScript ExportAll.java "$OUT/server" > "$OUT/server/export.log" 2>&1
  echo "server.dll: $OUT/server/decomp.c"
fi
echo "decompiled source: $OUT/src (index: $OUT/src/index.tsv)"
