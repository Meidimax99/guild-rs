#!/usr/bin/env python3
"""Dump the AI action ("method") table: run the four registration functions of
GildeGold_TL.exe in the emulator and read the 0x94-byte records at 0xfb0080.

    conda run -n guild python re/scripts/ai_methods.py <path/to/GildeGold_TL.exe>

Prints a TSV: id, name, score function, target function, execute function,
flags (the drive weights come from gamedata/ai/ai_data_2.dfn instead).
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from difftest import Exe  # noqa: E402

TABLE = 0xFB0080
RECORD = 0x94
COUNT = 0x49
REGISTER = [0x48FB60, 0x47F080, 0x47DFE0, 0x49A330]


def main():
    exe = Exe(sys.argv[1])
    names = {}
    for line in open(os.path.join(os.path.dirname(__file__), "..", "names.tsv")):
        p = line.rstrip("\n").split("\t")
        if len(p) >= 2 and len(p[0]) == 8:
            names[int(p[0], 16)] = p[1]
    for f in REGISTER:
        if exe.call(f, 0) == 0:
            raise SystemExit(f"registration {f:#x} failed")
    print("id\tname\tscore\ttarget\texecute\tflags")
    for i in range(COUNT):
        rec = bytes(exe.uc.mem_read(TABLE + i * RECORD, RECORD))
        if rec[0] != i:
            continue
        name = rec[1:0x21].split(b"\0")[0].decode("latin-1")
        fns = struct.unpack_from("<III", rec, 0x24)
        flags = struct.unpack_from("<I", rec, 0x90)[0]
        fn = lambda a: names.get(a, f"FUN_{a:08x}") if a else "-"
        print(f"{i}\t{name}\t" + "\t".join(fn(a) for a in fns) + f"\t{flags:#x}")


if __name__ == "__main__":
    main()
