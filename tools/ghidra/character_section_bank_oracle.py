"""Capture original 12340 character section bank construction and lookup.

Only allocation (76E540) is replaced with mapped memory. The unchanged
4F3DD0 builds the table and 4F3BA0 selects each row; no class/flag policy is
supplied by the harness. The optional installed DBC remains a local input.
"""
import argparse
from pathlib import Path
import struct
import wmo_registration_oracle as n
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(records, queries):
    u = n.emulator()
    arena = 0x05000000
    u.mem_map(arena, 0x1000000)
    u.mem_write(arena, b''.join(struct.pack('<10I', *row) for row in records))
    n.write_words(u, 0xad3334, len(records))
    n.write_words(u, 0xad3348, arena)
    next_alloc = arena + 0x400000
    def allocate(uc, address, size, context):
        nonlocal next_alloc
        if address != 0x76e540:
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret, count = n.read_words(uc, sp, 2)
        assert next_alloc + count < arena + 0x1000000
        uc.reg_write(UC_X86_REG_EAX, next_alloc)
        next_alloc += (count + 15) & ~15
        uc.reg_write(UC_X86_REG_ESP, sp + 4)
        uc.reg_write(UC_X86_REG_EIP, ret)
    u.hook_add(UC_HOOK_CODE, allocate, begin=0x76e540, end=0x76e540)
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, (max(row[1] for row in records) + 1) * 2, n.HEAP)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x4f3dd0, n.STOP, timeout=60_000_000, count=20_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP
    assert u.reg_read(UC_X86_REG_EAX) == 1
    bank = n.read_words(u, n.HEAP, 1)[0]
    output = []
    for race, gender, base, variation, color in queries:
        n.invoke(u, 0x4f3ba0, [bank, race, gender, base, variation, color, 0])
        row = u.reg_read(UC_X86_REG_EAX)
        output.append(n.read_words(u, row, 1)[0] if row else 0)
    return output

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    parser.add_argument('--dbc', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    records, queries = [], []
    for base in range(5):
        for flags in range(32):
            for ident, flag in [(1000 + base * 32 + flags, 1), (2000 + base * 32 + flags, flags)]:
                records.append([ident, 1, 0, base, 0, 0, 0, flag, 0, flags])
            queries.append([1, 0, base, 0, flags])
    results = capture(records, queries)
    lines = ['# Original 12340 4F3DD0 -> 4F3BA0; allocation only hooked.', '# race gender base variation color finalFlags selectedRowId']
    for query, result in zip(queries, results):
        lines.append(' '.join(map(str, [*query, query[4], result])))
    args.output.write_text('\n'.join(lines) + '\n')
    print(f'Captured {len(results)} native section lookups')
    if args.dbc:
        data = args.dbc.read_bytes()
        count, fields, stride = struct.unpack_from('<3I', data, 4)
        assert fields == 10 and stride == 40
        records = [list(struct.unpack_from('<10I', data, 20 + i*stride)) for i in range(count)]
        queries = [[2, 0, base, 0, 17] for base in [0, 1, 4]]
        print('Installed Orc male skin17:', list(zip(queries, capture(records, queries))))
