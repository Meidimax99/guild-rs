"""Decode SHAPBANK sprites from a Die Gilde .gfx archive to PNG (for verification)."""
import struct, sys, zlib, os

def png(path, w, h, rgba):
    raw = b''.join(b'\0' + rgba[y*w*4:(y+1)*w*4] for y in range(h))
    def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    open(path, 'wb').write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))

def frame(bank, i):
    off = struct.unpack_from('<I', bank, 0x45 + 4 * i)[0]
    f = off
    w = struct.unpack_from('<H', bank, f + 6)[0]
    h = struct.unpack_from('<H', bank, f + 10)[0]
    has = struct.unpack_from('<i', bank, f + 0x26)[0]
    rgba = bytearray(w * h * 4)
    if has == -1:
        return w, h, bytes(rgba)
    p = f + 0x32
    for y in range(h):
        n = struct.unpack_from('<I', bank, p)[0]; p += 4
        x = 0
        for _ in range(n):
            skip, cnt = struct.unpack_from('<II', bank, p); p += 8
            x += skip // 3
            for k in range(cnt):
                r, g, b = bank[p], bank[p+1], bank[p+2]; p += 3
                if 0 <= x < w:
                    a = 0xc0 if (cnt >= 4 and k == 0) or k == cnt - 1 else 0xff
                    rgba[(y*w+x)*4:(y*w+x)*4+4] = bytes((r, g, b, a))
                x += 1
    return w, h, bytes(rgba)

d = open(sys.argv[1], 'rb').read()
n = struct.unpack_from('<I', d, 0)[0]
want = sys.argv[3:]
for i in range(n):
    o = 4 + i * 0x54
    name = d[o:o+0x30].split(b'\0')[0].decode('cp1252')
    off, _, size = struct.unpack_from('<III', d, o + 0x30)
    if size == 0 or (want and name not in want):
        continue
    bank = d[off:off+size]
    count = struct.unpack_from('<H', bank, 0x2a)[0]
    print(name, 'frames', count, 'maxw/h', struct.unpack_from('<HH', bank, 0x2c), 'fmt', bank[8:10].hex())
    for k in range(min(count, 4)):
        w, h, rgba = frame(bank, k)
        png(os.path.join(sys.argv[2], f'{name}_{k}.png'), w, h, rgba)
