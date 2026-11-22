#!/usr/bin/env python3
"""Differential test vectors: run original functions of GildeGold_TL.exe in an
x86 emulator (Unicorn) and record inputs and outputs as JSON. The Rust tests in
crates/gilde-sim/tests/difftest.rs replay the vectors against the port.

    conda run -n guild python re/scripts/difftest.py <path/to/GildeGold_TL.exe> [outdir]

(the `guild` conda environment has `unicorn` installed; elsewhere
`pip install unicorn`).

outdir defaults to crates/gilde-sim/tests/data. The functions used are pure
(they only touch their arguments, a few globals and the CRT's __ftol), so no
Windows environment is needed.
"""
import json
import os
import random
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_MEM_UNMAPPED
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX

PAGE = 0x1000
STACK = 0x0020_0000
STACK_SIZE = 0x10_0000
RETURN = 0x0010_0000  # sentinel return address (mapped, never executed)
FPU_STUB = RETURN + 0x800  # fninit; fldcw [FPU_CW]
FPU_CW = RETURN + 0x900
# The game creates its Direct3D 8 device without D3DCREATE_FPU_PRESERVE
# (behaviour flags 0x50/0x40/0x20), so Direct3D switches the x87 FPU to
# 24-bit precision: the simulation runs with control word 0x007F.
GAME_FPU_CW = 0x007F

# Addresses (see re/names.tsv).
DATE_ADD = 0x4EC320
DATE_MINUTES = 0x4EC460
DRIVES_ONE = 0x46BF20
CLK_ADVANCE = 0x51F960
G_CHARACTERS = 0x79CD40
G_DATE = 0x79CCA0
CLK_DATE = 0x78A2F0
CLK_SPEED = 0x71F7C8
SESSION_FLAGS = 0x71D8D0


def align(x, a=PAGE):
    return (x + a - 1) & ~(a - 1)


class Exe:
    def __init__(self, path):
        d = open(path, "rb").read()
        pe = struct.unpack_from("<I", d, 0x3C)[0]
        base = struct.unpack_from("<I", d, pe + 24 + 28)[0]
        nsec = struct.unpack_from("<H", d, pe + 6)[0]
        osz = struct.unpack_from("<H", d, pe + 20)[0]
        self.uc = Uc(UC_ARCH_X86, UC_MODE_32)
        lo, hi = None, 0
        secs = []
        for i in range(nsec):
            o = pe + 24 + osz + 40 * i
            vs, va, rs, ro = struct.unpack_from("<IIII", d, o + 8)
            secs.append((base + va, max(vs, rs), d[ro:ro + rs]))
            lo = base + va if lo is None else min(lo, base + va)
            hi = max(hi, base + va + max(vs, rs))
        self.uc.mem_map(base, align(hi - base))
        self.uc.mem_write(base, d[:min(len(d), PAGE)])
        for va, size, data in secs:
            self.uc.mem_write(va, data)
        self.uc.mem_map(STACK, STACK_SIZE)
        self.uc.mem_map(RETURN, PAGE)
        self.uc.mem_write(FPU_STUB, b"\xdb\xe3\xd9\x2d" + struct.pack("<I", FPU_CW))
        self.uc.mem_write(FPU_CW, struct.pack("<H", GAME_FPU_CW))
        self.image = bytes(self.uc.mem_read(base, align(hi - base)))
        self.base = base

        def unmapped(uc, access, address, size, value, data):
            raise RuntimeError(f"unmapped access at {address:#x}")

        self.uc.hook_add(UC_HOOK_MEM_UNMAPPED, unmapped)

    def reset(self):
        """Restore the original image (globals) between calls."""
        self.uc.mem_write(self.base, self.image)

    def call(self, addr, *args):
        # Defined FPU state for every call (registers, tags, control word).
        self.uc.emu_start(FPU_STUB, FPU_STUB + 8)
        sp = STACK + STACK_SIZE - 0x100
        for a in reversed(args):
            sp -= 4
            self.uc.mem_write(sp, struct.pack("<I", a & 0xFFFFFFFF))
        sp -= 4
        self.uc.mem_write(sp, struct.pack("<I", RETURN))
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        self.uc.emu_start(addr, RETURN, count=2_000_000)
        return self.uc.reg_read(UC_X86_REG_EAX)

    def write(self, addr, fmt, *values):
        self.uc.mem_write(addr, struct.pack(fmt, *values))

    def read(self, addr, fmt):
        return struct.unpack(fmt, bytes(self.uc.mem_read(addr, struct.calcsize(fmt))))


DATE_FMT = "<iHii"


def rand_date(r):
    return [r.randint(0, 200), r.randint(0, 23), r.randint(0, 59), r.randint(0, 59)]


def date_add_vectors(exe, r, n=400):
    out = []
    for _ in range(n):
        d = rand_date(r)
        h, m, s = r.randint(-30, 30), r.randint(-200, 200), r.randint(-5000, 5000)
        exe.reset()
        exe.write(CLK_DATE, DATE_FMT, *d)
        exe.call(DATE_ADD, CLK_DATE, h, m, s)
        out.append({"date": d, "add": [h, m, s], "result": list(exe.read(CLK_DATE, DATE_FMT))})
    return out


def minutes_vectors(exe, r, n=200):
    out = []
    for _ in range(n):
        a, b = rand_date(r), rand_date(r)
        exe.reset()
        exe.write(CLK_DATE, DATE_FMT, *a)
        exe.write(G_DATE, DATE_FMT, *b)
        res = exe.call(DATE_MINUTES, CLK_DATE, G_DATE)
        out.append({"a": a, "b": b, "minutes": struct.unpack("<i", struct.pack("<I", res))[0]})
    return out


def drives_vectors(exe, r, n=300):
    out = []
    ch = G_CHARACTERS + 0x21C * 10
    for _ in range(n):
        drives = []
        for i in range(14):
            rate = r.choice([0.0, r.uniform(0, 5)])
            decay = r.choice([0.0, r.uniform(0, 0.01), r.uniform(0, 0.2)])
            value = r.choice([0.0, 1000.0, r.uniform(0, 1000)])
            drives.append([rate, decay, value])
        ctx = {
            "activity": r.randint(0, 12),
            "action_points": r.randint(0, 12),
            "residence": r.choice([0, 0, r.randint(1, 400)]),
            "kind": r.choice([0, 1, 3, 5, 6, 7]),
            "greed": r.uniform(0.5, 1.5),
        }
        exe.reset()
        exe.write(ch + 0x02, "<B", ctx["kind"])
        for i, (a, b, c) in enumerate(drives):
            exe.write(ch + 0x88 + 12 * i, "<fff", a, b, c)
        exe.write(ch + 0x130, "<B", ctx["activity"])
        exe.write(ch + 0x170, "<i", ctx["residence"])
        exe.write(ch + 0x194, "<i", ctx["action_points"])
        exe.write(ch + 0x1E4, "<f", ctx["greed"])
        # Round-trip the inputs through f32 so both sides start identical.
        drives = [list(exe.read(ch + 0x88 + 12 * i, "<fff")) for i in range(14)]
        ctx["greed"] = exe.read(ch + 0x1E4, "<f")[0]
        exe.call(DRIVES_ONE, ch)
        after = [list(exe.read(ch + 0x88 + 12 * i, "<fff")) for i in range(14)]
        out.append({
            "drives": drives,
            "ctx": ctx,
            "result": after,
            "activity": exe.read(ch + 0x130, "<B")[0],
        })
    return out


def clock_vectors(exe, r, n=200):
    out = []
    for _ in range(n):
        d = rand_date(r)
        g = list(d)
        if r.random() < 0.3:
            g[1] = r.choice([22, 23])
        speed = r.choice([0, 40, 80, 120, 160, 750, r.randint(0, 800)])
        flags = r.choice([0x01, 0x05, 0x89])
        exe.reset()
        exe.write(CLK_DATE, DATE_FMT, *d)
        exe.write(G_DATE, DATE_FMT, *g)
        exe.write(CLK_SPEED, "<i", speed)
        exe.write(SESSION_FLAGS, "<I", flags)
        exe.call(CLK_ADVANCE)
        out.append({"date": d, "game_date": g, "speed": speed, "flags": flags,
                    "result": list(exe.read(CLK_DATE, DATE_FMT))})
    return out


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    exe = Exe(sys.argv[1])
    here = os.path.dirname(os.path.abspath(__file__))
    outdir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, "../../crates/gilde-sim/tests/data")
    os.makedirs(outdir, exist_ok=True)
    r = random.Random(1419)
    for name, fn in [("date_add", date_add_vectors), ("date_minutes", minutes_vectors),
                     ("drives", drives_vectors), ("clock", clock_vectors)]:
        vectors = fn(exe, r)
        path = os.path.join(outdir, f"{name}.json")
        with open(path, "w") as f:
            json.dump(vectors, f, separators=(",", ":"))
        print(f"{name}: {len(vectors)} vectors -> {path}")


if __name__ == "__main__":
    main()
