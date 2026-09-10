"""Execute 54AE40's arena-context updates with synthetic packet/DBC providers.

Only packet reads, time and UI/sound notifications are hooked. Queue gating,
status branches, Map.dbc lookup and the cached arena/active-queue stores run
the fingerprinted client's original instructions.
"""
import argparse
import itertools
from pathlib import Path
import struct
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    u = n.emulator()
    reader, map_bank, map_row = [n.HEAP + i * 0x1000 for i in range(3)]
    body = b''
    n.write_words(u, 0xad4170, 40)
    n.write_words(u, 0xad416c, 43)
    n.write_words(u, 0xad4180, map_bank)
    n.write_words(u, map_bank, map_row, map_row + 16, map_row + 32, 0)
    for index, kind in enumerate([0, 3, 4]):
        n.write_words(u, map_row + index * 16 + 8, kind)
    n.write_words(u, 0xad4f78, 0)

    def hook(u, address, size, context):
        if address in (0x47b340, 0x47b3c0, 0x47b400):
            length = {0x47b340: 1, 0x47b3c0: 4, 0x47b400: 8}[address]
            destination = n.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 1)[0]
            cursor = n.read_words(u, reader + 0x14, 1)[0]
            assert cursor + length <= len(body)
            u.mem_write(destination, body[cursor:cursor + length])
            n.write_words(u, reader + 0x14, cursor + length)
            return_value(u, reader)
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + 4)
        elif address in (0x848140, 0x8483c0, 0x530840, 0x54aa30, 0x52e9f0,
                         0x84a6e0, 0x81b530, 0x86ae20):
            return_value(u, 12345 if address == 0x86ae20 else 0)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# 54AE40: initial-active initial-kind packet-hex final-active final-kind consumed-bytes.']
    variants = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (1, 2, 42),
                *[(1, 3, map_id) for map_id in [39, 40, 41, 42, 43, 44]],
                (1 << 32, 3, 42), (1, 99, 0)]
    for queue, active, kind, (guid, status, map_id), tail in itertools.product(
            [0, 1, 2, 0xffffffff], [0, 1, 0xffffffff], [0, 3, 4], variants, [False, True]):
        body = struct.pack('<I', queue)
        if queue < 2:
            body += struct.pack('<Q', guid)
            if guid:
                body += struct.pack('<BBIBI', 10, 80, 987, 255, status)
                if status == 1:
                    body += struct.pack('<II', 456, 789)
                elif status == 2:
                    body += struct.pack('<IQI', map_id, 0xabcdef0123456789, 123)
                elif status == 3:
                    body += struct.pack('<IQIIB', map_id, 0xabcdef0123456789, 123, 456, 255)
        if tail:
            body += b'\xab\xcd\xef'
        n.write_words(u, reader + 0x10, len(body), 0)
        n.write_words(u, 0xacd16c, active)
        n.write_words(u, 0xbea570, kind)
        n.invoke(u, 0x54ae40, [0, 0, 0, reader])
        result_active = n.read_words(u, 0xacd16c, 1)[0]
        result_kind = n.read_words(u, 0xbea570, 1)[0]
        consumed = n.read_words(u, reader + 0x14, 1)[0]
        rows.append(f'context {active} {kind} {body.hex()} {result_active} {result_kind} {consumed}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    Path(args.output).write_text('\n'.join(capture()) + '\n')
