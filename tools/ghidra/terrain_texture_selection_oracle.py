"""Capture stock 7D6980 terrain filenames with controlled GPU capabilities.

Runs the fingerprinted client code in Unicorn. GPU capability lookup and the
texture request are supplied boundaries; filename rewriting executes native
76ED20/76E720. No client process or archive loader is executed.
"""
import argparse
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(path, flags, enabled):
    u = n.emulator()
    owner, entry, name, table, caps = [n.HEAP + i * 0x1000 for i in range(5)]
    u.mem_write(name, path.encode() + b'\0')
    n.write_words(u, entry, name, 0)
    n.write_words(u, owner + 0xb4, table if flags is not None else 0)
    n.write_words(u, table + 12, flags or 0)
    for offset in (0x5c, 0xb4, 0xc4):
        n.write_words(u, caps + offset, 1)
    u.mem_write(0xce049d, bytes([enabled]))
    selected = []

    def hook(uc, address, size, data):
        if address not in (0x532af0, 0x7d9990):
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret = n.read_words(uc, sp, 1)[0]
        if address == 0x532af0:
            result = caps
        else:
            pointer = n.read_words(uc, sp + 4, 1)[0]
            selected.append(bytes(uc.mem_read(pointer, 260)).split(b'\0')[0].decode())
            result = 0x12345678
        uc.reg_write(UC_X86_REG_EAX, result)
        uc.reg_write(UC_X86_REG_ESP, sp + 4)
        uc.reg_write(UC_X86_REG_EIP, ret)

    u.hook_add(UC_HOOK_CODE, hook)
    u.reg_write(UC_X86_REG_ECX, owner)
    n.invoke(u, 0x7d6980, [entry, 3])
    assert n.read_words(u, entry + 4, 1)[0] == 0x12345678
    assert len(selected) == 1
    return selected[0]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# specular MTXF-or-absent MTEX selected-texture; native 7D6980']
    for path in ('tileset/fixture/road.blp', 'tileset/fixture/rock.detail.blp'):
        for flags in (None, 0, 1, 2, 3, 0xffffffff):
            for enabled in (0, 1):
                selected = capture(path, flags, enabled)
                rows.append(f'{enabled} {flags if flags is not None else "absent"} {path} {selected}')
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native terrain texture selections')
