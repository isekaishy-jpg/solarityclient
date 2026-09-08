"""Capture build-12340 mirror-timer receipt, event ordering and Lua progress.

Executes original 519A50 and its actual CDataStore readers, 5199A0 and its
resident Spell.dbc lookup, 513EA0 reset, and 517AA0 progress/token arithmetic.
Hooks only client time, Lua argument/result/event bridges, the localized-label
formatter/provider, and tutorial dispatch. No OS or visible application runs.
Requires the fingerprinted owned PE and Unicorn; output is fixed 296-byte cases.
"""

import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value

DELTAS = (0, 1, 1234, 100000, 0x80000001)
TOKENS = ('EXHAUSTION', 'BREATH', 'FEIGNDEATH', 'UNKNOWN')


def cstring(uc, address):
    return bytes(uc.mem_read(address, 128)).split(b'\0')[0].decode()


def capture():
    uc = native.emulator()
    packet, payload, spell_index, spell_row, spell_name, label, query = [native.HEAP + x for x in (0, 0x100, 0x200, 0x300, 0x600, 0x700, 0x800)]
    native.write_words(uc, 0xad49d0 + 0xc, 5384, 5384)
    native.write_words(uc, 0xad49d0 + 0x20, spell_index)
    native.write_words(uc, spell_index, spell_row)
    native.write_words(uc, spell_row + 0x220, spell_name)
    uc.mem_write(spell_name, b'Authored spell\0')
    uc.mem_write(0xc5dea0, b'\0')
    native.invoke(uc, 0x9c98a0, [])
    now, result, events, tutorials = 0, [], [], []

    def provider(uc, address, size, context):
        nonlocal result
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x86ae20:
            return_value(uc, now)
        elif address == 0x76f070:
            output, capacity, fmt, token = native.read_words(uc, sp + 4, 4)
            assert cstring(uc, fmt) == '%s_LABEL'
            text = (cstring(uc, token) + '_LABEL').encode()
            assert len(text) + 1 <= capacity
            uc.mem_write(output, text + b'\0')
            return_value(uc, len(text))
        elif address == 0x819d40:
            token = cstring(uc, native.read_words(uc, sp + 4, 1)[0])
            text = {'BREATH_LABEL': 'Breath', 'EXHAUSTION_LABEL': 'Fatigue'}.get(token, '')
            uc.mem_write(label, text.encode() + b'\0')
            return_value(uc, label)
        elif address == 0x530840:
            tutorials.append(native.read_words(uc, sp + 4, 1)[0])
            return_value(uc, 0)
        elif address == 0x81b530:
            event, fmt = native.read_words(uc, sp + 4, 2)
            count = {0x160: 6, 0x161: 2, 0x162: 1}[event]
            args = list(native.read_words(uc, sp + 12, count))
            args[0] = TOKENS.index(cstring(uc, args[0]))
            if event == 0x160:
                args[5] = ['', 'Fatigue', 'Breath', 'Authored spell'].index(cstring(uc, args[5]))
            events.append(([event] + args + [0]*(6-count), bytes(uc.mem_read(0xbd0b80, 84))))
            return_value(uc, 0)
        elif address == 0x84df60:
            return_value(uc, 1)
        elif address == 0x84e0e0:
            return_value(uc, query)
        elif address == 0x84e2a0:
            result.append(int(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0]))
            return_value(uc, 0)
        elif address == 0x84f280:
            raise AssertionError('Unexpected native Lua argument failure')

    uc.hook_add(UC_HOOK_CODE, provider)

    def start(timer, value, maximum, scale, paused=0, spell=0):
        return 0x1d9, struct.pack('<IiiiBI', timer, value, maximum, scale, paused, spell)

    cases = [
        start(1, 60000, 60000, -1),
        start(0, 45000, 60000, -1),
        start(2, 360000, 360000, -1, 0, 5384),
        (0x1da, struct.pack('<IB', 1, 1)),
        (0x1da, struct.pack('<IB', 1, 0)),
        start(1, 14000, 60000, 10),
        start(1, -5, -1, -17, 255),
        start(0, 0x7ffffffe, 0x7fffffff, 0x7fffffff, 2),
        start(2, -0x80000000, 10, -0x80000000),
        start(1, 60000, 60000, 0, 1),
        start(0xffffffff, 123, 456, 7, 2, 5384),
        (0x1da, struct.pack('<IB', 99, 255)),
        (0x1db, struct.pack('<I', 99)),
        (0x1db, struct.pack('<I', 1)),
        (0x1db, struct.pack('<I', 1)),
        start(1, 20000, 60000, -1, 0, 999),
        start(2, 20000, 60000, 7, 1, 5384),
        (0x1db, struct.pack('<I', 0)),
        (0x1db, struct.pack('<I', 2)),
        (0x1db, struct.pack('<I', 1)),
    ]
    records = []
    for repeat in range(2):
        for index, (opcode, body) in enumerate(cases):
            now = (10000 + index*431 if repeat == 0 else 0xfffff000 + index*1234) & 0xffffffff
            timestamp = now
            before = bytes(uc.mem_read(0xbd0b80, 84))
            events.clear()
            tutorials.clear()
            uc.mem_write(payload, body)
            native.write_words(uc, packet, 0, payload, 0, len(body), len(body), 0)
            native.invoke(uc, 0x519a50, [0, opcode, 0, packet])
            assert native.read_words(uc, packet + 0x14, 1)[0] == len(body)
            assert len(events) == 1 and events[0][1] == before
            after = bytes(uc.mem_read(0xbd0b80, 84))
            result = []
            for delta in DELTAS:
                now = (timestamp + delta) & 0xffffffff
                for token in TOKENS[:3]:
                    uc.mem_write(query, token.lower().encode() + b'\0')
                    native.invoke(uc, 0x517aa0, [0])
            record = (struct.pack('<III', timestamp, opcode, len(body)) + body.ljust(24, b'\0')
                      + after + struct.pack('<7I', *events[0][0])
                      + struct.pack('<I', tutorials[0] if tutorials else 0xffffffff)
                      + struct.pack('<15i', *result) + events[0][1])
            assert len(record) == 296
            records.append(record)
    events.clear()
    native.invoke(uc, 0x513ea0, [])
    assert [event[0][:2] for event in events] == [[0x162, 0], [0x162, 1], [0x162, 2]]
    assert native.read_words(uc, 0xbd0b80, 21) == tuple([3, 0, 0, 0, 0, 0, 0]*3)
    return b''.join(records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    data = capture()
    Path(args.output).write_bytes(data)
    print(f'{len(data)//296} original receiver/event cases and {len(data)//296*15} Lua progress captures')


if __name__ == '__main__':
    main()
