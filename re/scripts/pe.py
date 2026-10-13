"""Minimal PE image reader: map virtual addresses to file bytes."""
import struct, sys

class PE:
    def __init__(self, path):
        self.data = open(path, 'rb').read()
        d = self.data
        pe = struct.unpack_from('<I', d, 0x3c)[0]
        nsec = struct.unpack_from('<H', d, pe + 6)[0]
        optsz = struct.unpack_from('<H', d, pe + 20)[0]
        self.base = struct.unpack_from('<I', d, pe + 24 + 28)[0]
        self.secs = []
        off = pe + 24 + optsz
        for i in range(nsec):
            name = d[off:off+8].rstrip(b'\0').decode()
            vsz, va, rsz, rptr = struct.unpack_from('<IIII', d, off + 8)
            self.secs.append((name, self.base + va, vsz, rsz, rptr))
            off += 40

    def read(self, va, n):
        for name, sva, vsz, rsz, rptr in self.secs:
            if sva <= va < sva + max(vsz, rsz):
                o = va - sva
                if o >= rsz:
                    return b'\0' * n
                return self.data[rptr + o: rptr + min(o + n, rsz)].ljust(n, b'\0')
        raise KeyError(hex(va))

    def u32(self, va):
        return struct.unpack('<I', self.read(va, 4))[0]

    def cstr(self, va):
        b = self.read(va, 256)
        return b.split(b'\0')[0].decode('latin1')
