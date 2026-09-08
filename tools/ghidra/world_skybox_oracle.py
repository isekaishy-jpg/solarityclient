"""Capture native LightSkybox slot compaction and realm-clock sequence requests.

Only DBC/model residency and the animation request sink are supplied. The
selection block and 7ECF20's cache/threshold/x87 arithmetic run unchanged.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_EDI, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture():
    u = n.emulator()
    entry, model, table = n.HEAP, n.HEAP + 0x100, n.HEAP + 0x1000
    requests = []
    duration, ready = 0, 1

    def ret(value, count):
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + count * 4)

    def hook(u, address, size, context):
        stack = u.reg_read(UC_X86_REG_ESP)
        if address == 0x824fc0:
            ret(ready, 2)
        elif address == 0x8266b0:
            _, _, out = n.read_words(u, stack, 3)
            u.mem_write(out, bytes(32))
            n.write_words(u, out + 0x14, duration)
            ret(1, 2)
        elif address == 0x832ab0:
            requests.append(n.read_words(u, stack + 4, 7))
            ret(1, 7)
        elif address == 0x7f30c0:
            return_value(u, u.reg_read(UC_X86_REG_EDI))
        elif address == 0x7f38cc:
            u.reg_write(UC_X86_REG_EIP, n.STOP)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Native skybox phase: duration flags ready minute cached-duration last-minute request(7 words or -).']
    for duration, flags, ready in itertools.product([0, 1, 1000, 83000, 86400000, 0x7fffffff, 0x80000001, 0xffffffff], [0, 1, 2, 3], [0, 1]):
        u.mem_write(entry, bytes(0x40))
        n.write_words(u, entry + 0x18, model, 0, 0, flags)
        rows.append('reset')
        for minute in [0, 1, 2, 3, 10, 10, 11, 13, 720, 1438, 1439, 0, 1, 3, -2, -2147483648, 2147483647]:
            requests.clear()
            n.invoke(u, 0x7ecf20, [entry, minute & 0xffffffff])
            cached, last = n.read_words(u, entry + 0x1c, 2)
            assert len(requests) <= 1
            request = ' '.join(f'{v:08x}' for v in requests[0]) if requests else '-'
            rows.append(f'phase {duration} {flags} {ready} {minute} {cached} {last:08x} {request}')
    # Valid ordinary/overlay/timed rows, an empty model path and a missing row.
    n.write_words(u, 0xaf49a8, 1)
    n.write_words(u, 0xaf49a4, 5)
    n.write_words(u, 0xaf49b8, table)
    for i, flags in enumerate([0, 2, 1]):
        row = table + 0x100 + i * 0x10
        n.write_words(u, table + i * 4, row)
        n.write_words(u, row, i + 1, i + 100, flags)
    n.write_words(u, table + 12, table + 0x130)
    n.write_words(u, table + 0x130, 4, 0, 0)
    n.write_words(u, table + 16, 0)
    edge = bits(.99)
    for ids in itertools.product(range(7), repeat=3):
        for weights in [(bits(.25), bits(.5), bits(.75)), (bits(1.), bits(1.), bits(1.)), (bits(-1.), 0, bits(1.))] + [(bits(.4), w, bits(.7)) for w in [edge - 1, edge, edge + 1]]:
            u.mem_write(0xd38b64, bytes(36))
            for i in range(3):
                n.write_words(u, 0xd38c50 + i * 8, ids[i], weights[i])
            u.reg_write(UC_X86_REG_EBP, n.STACK + 0x10000)
            n.invoke(u, 0x7f3809, [])
            # Native clears all pointers before the block and only the first
            # unused weight afterward. A reset can leave stale trailing pointers;
            # consumption still sees them, including their previous flags.
            output = n.read_words(u, 0xd38b64, 9)
            rows.append('slots ' + ' '.join(map(str, ids)) + ' ' + ' '.join(f'{v:08x}' for v in weights + tuple(output)))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
