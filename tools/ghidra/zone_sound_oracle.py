"""Capture native area/WMO sound inheritance without reimplementing its decisions.

The complete 0x0078e9a0 function executes. Only downstream sound setters and
the world-state override lookup are intercepted to record selected relations.
The executable fingerprint and PE mapping come from the shared native harness.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_EBX, UC_X86_REG_EBP, UC_X86_REG_ESI, UC_X86_REG_ECX


def capture(executable, output):
    """Run sparse parent/child source combinations through the original code."""
    native.initialize(executable)
    uc = native.emulator()
    selected = [0] * 5
    setters = {0x4c86b0: 0, 0x4c86d0: 1, 0x4c8c90: 2, 0x4c8d80: 3, 0x4c8cf0: 4}

    def dependencies(u, address, _size, _data):
        """Observe downstream calls; never calculate a selected relation here."""
        if address not in setters and address not in (0x4cbe70, 0x4c8600):
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in setters:
            offset = 4 if address == 0x4c86d0 else 8
            selected[setters[address]] = native.read_words(u, sp + offset, 1)[0]
        u.reg_write(UC_X86_REG_EAX, 0)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    rng = random.Random(12340)
    rows = ['# native 0078e9a0: present-mask wmo-only four sources*five fields -> five selected fields']
    for mask in range(16):
        for wmo_only in (0, 1):
            for case in range(10):
                sources = [[(source + 1) * 100 + field + 1 if case == 0 or rng.randrange(3) == 0 else 0 for field in range(5)] for source in range(4)]
                args = []
                for index, values in enumerate(sources):
                    pointer = native.HEAP + index * 0x1000
                    uc.mem_write(pointer, bytes(0x100))
                    native.write_words(uc, pointer, index + 1)
                    native.write_words(uc, pointer + (20 if index < 2 else 16), *values)
                    args.append(pointer if mask & (1 << index) else 0)
                selected[:] = [0] * 5
                invoke(uc, 0x78e9a0, args + [wmo_only])
                values = [mask, wmo_only] + [value for source in sources for value in source] + selected
                rows.append(' '.join(map(str, values)))
    # Run the original comparator with the fields used by 4C9850.
    for minute in (0, 329, 330, 331, 1259, 1260, 1261, 1439):
        native.write_words(uc, native.HEAP, minute % 60, minute // 60, 0xffffffff, 0, 0, 0, 0)
        native.write_words(uc, native.HEAP + 128, 30, 5, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0)
        native.write_words(uc, native.HEAP + 256, 0, 21, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0)
        uc.reg_write(UC_X86_REG_ECX, native.HEAP)
        invoke(uc, 0x76cf10, [native.HEAP + 128, native.HEAP + 256])
        rows.append(f'# time {minute} {uc.reg_read(UC_X86_REG_EAX)}')
    # Execute each complete fade arithmetic branch, stopping before downstream
    # resource callbacks. FLDZ supplies the enclosing native loop's zero.
    bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    voice, frame = native.HEAP, native.STACK + 0x10000
    uc.mem_write(native.STOP + 32, b'\xdb\xe3\xd9\xee')
    for direction in (0, 1):
        for gain in (0., .125, .5, 1.):
            for duration in (.01, .5, 4., 5.):
                for elapsed in (0., .001, .1, 1., 20.):
                    native.write_words(uc, voice + 0x24, bits(gain), bits(duration), bits(duration))
                    uc.mem_write(voice + 0x30, bytes([1 - direction, direction]))
                    native.write_words(uc, frame - 8, bits(elapsed))
                    uc.reg_write(UC_X86_REG_EBX, voice)
                    uc.reg_write(UC_X86_REG_EBP, frame)
                    uc.reg_write(UC_X86_REG_ESI, 0)
                    uc.emu_start(native.STOP + 32, native.STOP + 36, count=2)
                    start, end = (0x87a4d1, 0x87a4f1) if direction == 0 else (0x87a425, 0x87a451)
                    uc.emu_start(start, end, count=100)
                    assert uc.reg_read(UC_X86_REG_EIP) == end
                    result = native.read_words(uc, voice + 0x24, 1)[0]
                    rows.append('# fade ' + ' '.join(f'{value:x}' for value in (direction, bits(gain), bits(duration), bits(elapsed), result)))
    # Execute the complete chunk address function with a controlled local player.
    chunk_uc = native.emulator()
    player, vtable = native.HEAP + 0x8000, native.HEAP + 0x8100
    native.write_words(chunk_uc, player, vtable)
    native.write_words(chunk_uc, vtable + 0x2c, native.STOP + 128)
    position = [0., 0., 0.]
    def chunk_dependencies(u, address, _size, _data):
        if address not in (0x77f080, 0x4d3790, 0x4d4db0, native.STOP + 128): return
        sp = u.reg_read(UC_X86_REG_ESP)
        consumed = 4
        if address == native.STOP + 128:
            native.write_floats(u, native.read_words(u, sp + 4, 1)[0], position)
            consumed = 8
        else:
            u.reg_write(UC_X86_REG_EAX, 530 if address == 0x77f080 else player)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + consumed)
    chunk_uc.hook_add(UC_HOOK_CODE, chunk_dependencies)
    for case in range(180):
        if case < 12:
            position[:2] = [(-17066.666, -33.333332, 0., 33.333332, 17066.666, 10000.)[case % 6], (-17066.666, 0., 17066.666)[case % 3]]
        else:
            position[:2] = [rng.uniform(-18000,18000), rng.uniform(-18000,18000)]
        position[:2] = [struct.unpack('<f', struct.pack('<f', value))[0] for value in position[:2]]
        invoke(chunk_uc, 0x4c6810, [native.HEAP + index * 4 for index in range(5)])
        rows.append('# chunk ' + ' '.join(f'{word:x}' for word in [bits(position[0]), bits(position[1]), *native.read_words(chunk_uc, native.HEAP, 5)]))

    # Keep state values and setter observation external; priority is native code.
    state_uc = native.emulator()
    state_values = [0] * 4
    def state_dependencies(u, address, _size, _data):
        if address not in setters and address != 0x548d10: return
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x548d10:
            key = native.read_words(u, sp + 4, 1)[0]
            result = state_values[key - 100] if 100 <= key < 104 else 0
        else:
            selected[setters[address]] = native.read_words(u, sp + 8, 1)[0]
            result = 0
        u.reg_write(UC_X86_REG_EAX, result)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)
    state_uc.hook_add(UC_HOOK_CODE, state_dependencies)
    native.write_words(state_uc, 0xb4ad78, 4, native.HEAP)
    for case in range(240):
        ids = [rng.choice([0, 11]), rng.choice([0, 12]), rng.choice([0, 21]), rng.choice([0, 22]), case % 2]
        state_values[:] = [rng.randrange(2) for _ in range(4)]
        records = []
        for index in range(4):
            record = [100 + index, rng.randrange(2), rng.choice([0, 11, 12]), rng.choice([0, 21, 22]), 201 + index, 301 + index, 401 + index, 501 + index]
            records += record
            ptr = native.HEAP + 0x100 + index * 0x100
            native.write_words(state_uc, native.HEAP + index * 4, ptr)
            native.write_words(state_uc, ptr, record[2], record[3], record[0], record[1], *record[4:])
        selected[:] = [0xffffffff] * 5
        invoke(state_uc, 0x4cbe70, ids)
        rows.append('# state ' + ' '.join(map(str, ids + state_values + records + [state_uc.reg_read(UC_X86_REG_EAX), selected[4], selected[3], selected[2], selected[0]])))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print('Captured 320 inheritance, 8 day/night, 160 fades, 180 chunks, and 240 world-state cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
