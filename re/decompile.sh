#!/bin/sh
# Reproduce the decompiled source of GildeGold_TL.exe outside the repository.
#
#   re/decompile.sh <path/to/GildeGold_TL.exe> [outdir]   (default outdir: ../re-out)
#
# Imports the executable into a headless Ghidra project (first run only, ~2 min),
# applies re/names.tsv, exports decomp.c / functions.tsv / strings.tsv and splits
# the decompile into per-module files under <outdir>/src/.
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
  "$HEADLESS" "$OUT" proj -import "$EXE" > "$OUT/import.log" 2>&1
fi
"$HEADLESS" "$OUT" proj -process "$(basename "$EXE")" -noanalysis -scriptPath "$S" \
  -postScript ApplyNames.java "$REPO/re/names.tsv" \
  -postScript ExportAll.java "$OUT" > "$OUT/export.log" 2>&1
python3 -I "$REPO/re/scripts/split_decomp.py" "$OUT/decomp.c" "$OUT/src"
echo "decompiled source: $OUT/src (index: $OUT/src/index.tsv)"
