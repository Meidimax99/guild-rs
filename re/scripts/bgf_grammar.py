"""Dump the table-driven chunk grammar used by the BGF/ED3 loaders.

Each table entry is 12 bytes: {u32 tag, u32 handler_or_subtable, u32 subtable_count}.
If subtable_count != 0 the entry descends into a nested table; otherwise the handler
function is called to consume the chunk payload. See FUN_00443530.
"""
import sys
sys.path.insert(0, __import__('os').path.dirname(__file__))
from pe import PE

pe = PE(sys.argv[1])
seen = set()
def dump(addr, count, depth):
    for i in range(count):
        e = addr + i * 12
        tag, h, n = pe.u32(e), pe.u32(e + 4), pe.u32(e + 8)
        if n:
            print('  ' * depth + f'tag 0x{tag:02x} -> table {h:#x}[{n}]')
            if (h, n) not in seen:
                seen.add((h, n)); dump(h, n, depth + 1)
        else:
            print('  ' * depth + f'tag 0x{tag:02x} -> handler {h:#x}')
dump(int(sys.argv[2], 16), int(sys.argv[3]), 0)
