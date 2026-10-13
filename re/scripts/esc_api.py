"""Extract ESC host-function registrations (FUN_004248a0 calls) from a Ghidra decompile dump.

Each call is FUN_004248a0(name, handler, return_type, argc, arg_type...).
Type codes: 1 int, 5 void, 6 string, 7 float (others printed raw).
Prints TSV: name, handler, ret, args, registering function.
"""
import re, sys
TYPES = {1: 'int', 5: 'void', 6: 'string', 7: 'float'}
src = open(sys.argv[1], encoding='latin1').read()
func = None
out = []
for line in src.splitlines():
    m = re.match(r'// ==== FUNCTION (\S+)', line)
    if m:
        func = m.group(1)
        continue
    m = re.search(r'FUN_004248a0\(([^;]*)\);', line)
    if not m:
        continue
    args = [a.strip() for a in m.group(1).split(',')]
    name = args[0]
    nm = re.match(r's_(.*)_[0-9a-f]{8}$', name)
    name = nm.group(1) if nm else name
    def ty(x):
        try:
            v = int(x, 0)
        except ValueError:
            return x
        return TYPES.get(v, str(v))
    ret = ty(args[2]) if len(args) > 2 else '?'
    params = [ty(a) for a in args[4:]]
    out.append((name, args[1], ret, ','.join(params), func))
for o in out:
    print('\t'.join(o))
