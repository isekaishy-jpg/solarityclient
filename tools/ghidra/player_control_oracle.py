"""Capture original pending-control arbitration and outgoing mover prefixes.

716060 runs without hooks. 71EF80 runs through its original opcode/GUID
writers and admission, stopping when it reaches MovementInfo serialization;
these captures establish the envelope, not snapshot construction or cipher I/O.
"""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP


def capture_release(executable, output):
    """Original 6E9980; hooks provide TLS clock and suppress unit notifications."""
    native.initialize(executable)
    uc = native.emulator()
    unit, clock = native.HEAP, native.HEAP + 0x1000
    def dependencies(u, address, _size, _data):
        if address not in [0x74b330, 0x7413f0]:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, clock if address == 0x74b330 else 0)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + (32 if address == 0x7413f0 else 0))
    uc.hook_add(UC_HOOK_CODE, dependencies)
    rows = ['# flags -> released flags (no spline; notification side effects excluded)']
    for axis in [0, 1, 2, 4, 8, 0x11, 0x24, 0xc0, 0xff]:
        for effects in [0, 0x100, 0x800, 0x100000, 0x1000, 0x3000, 0x20000000, 0x40000000]:
            for deferred in [0, 0xfc000, 0xc00000, 0x4000000]:
                flags = axis | effects | deferred
                uc.mem_write(unit, bytes(0x400))
                native.write_words(uc, unit + 0x28, unit + 0x300)
                native.write_words(uc, unit + 0x44, flags)
                uc.reg_write(UC_X86_REG_ECX, unit)
                invoke(uc, 0x6e9980, [])
                rows.append(f'{flags:x} {native.read_words(uc, unit + 0x44, 1)[0]:08x}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original release responses')


def capture_acquire(executable, output):
    """98B710 and all callees execute unmodified, with no active spline."""
    native.initialize(executable)
    uc = native.emulator()
    unit = native.HEAP
    rows = ['# flags secondary -> changed flags time launchHeight launchSpeed']
    for flags in [0, 1, 2, 4, 8, 0x31, 0xc0, 0x100, 0x200, 0x400, 0x800,
                  0x1000, 0x2000, 0x100000, 0x200000, 0x2000000, 0x4000000,
                  0x20000000, 0x40000000]:
        for secondary in [0, 4, 8, 0x20]:
            uc.mem_write(unit, bytes(0x400))
            native.write_words(uc, unit + 0x28, unit + 0x300)
            native.write_words(uc, unit + 0x44, flags, secondary)
            native.write_floats(uc, unit + 0x10, [3., 4., 5.])
            native.write_words(uc, unit + 0x80, 123, bits(9.))
            native.write_words(uc, unit + 0xb8, bits(-7.))
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, 0x98b710, [])
            changed = uc.reg_read(UC_X86_REG_EAX)
            values = [*native.read_words(uc, unit + 0x44, 1),
                      *native.read_words(uc, unit + 0x80, 2),
                      *native.read_words(uc, unit + 0xb8, 1)]
            rows.append(f'{flags:x} {secondary:x} {changed} ' + ' '.join(f'{v:08x}' for v in values))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original acquire-fall responses')


def capture(executable, pending_output, envelope_output):
    native.initialize(executable)
    uc = native.emulator()
    guids = [0, 1, 0x1234000056780000, 0xabcdef0123456789]
    rows = ['# previousGUID previousByte nextGUID nextByte -> retainedGUID retainedByte']
    for previous in guids:
        for previous_byte in [0, 1, 255]:
            for next_guid in guids:
                for next_byte in [0, 1, 255]:
                    native.write_words(uc, 0xca1248, previous & 0xffffffff, previous >> 32, previous_byte)
                    invoke(uc, 0x716060, [next_guid & 0xffffffff, next_guid >> 32, next_byte])
                    lo, hi, value = native.read_words(uc, 0xca1248, 3)
                    rows.append(f'{previous:016x} {previous_byte} {next_guid:016x} {next_byte} {(hi << 32) | lo:016x} {value & 255}')
    Path(pending_output).write_text('\n'.join(rows) + '\n', encoding='utf-8')

    unit, movement, guid_address, buffer, data = [native.HEAP + i * 0x2000 for i in range(5)]
    reached = False
    def stop_at_snapshot(u, address, _size, _data):
        nonlocal reached
        if address == 0x7164b0:
            reached = True
            u.reg_write(UC_X86_REG_EIP, native.STOP)
    uc.hook_add(UC_HOOK_CODE, stop_at_snapshot)
    native.write_words(uc, unit + 8, guid_address)
    native.write_words(uc, unit + 0xd8, movement)
    rows = ['# opcode GUID -> native bytes before MovementInfo (includes u32 opcode)']
    for opcode in [0xee, 0x2d1]:
        for guid in guids:
            native.write_words(uc, guid_address, guid & 0xffffffff, guid >> 32)
            native.write_words(uc, 0xca1238, guid & 0xffffffff, guid >> 32)
            native.write_words(uc, buffer, 0, data, 0, 0x1000, 0)
            reached = False
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, 0x71ef80, [123, opcode, buffer, 0, 0])
            assert reached, 'Native envelope did not reach MovementInfo'
            length = native.read_words(uc, buffer + 16, 1)[0]
            value = bytes(uc.mem_read(data, length))
            rows.append(f'{opcode:x} {guid:016x} {value.hex()}')
    Path(envelope_output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print('Captured 144 original pending-control cases and 8 original movement envelopes')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('pending_output'); parser.add_argument('envelope_output')
    parser.add_argument('--acquire-output')
    parser.add_argument('--release-output')
    args = parser.parse_args()
    capture(args.executable, args.pending_output, args.envelope_output)
    if args.acquire_output:
        capture_acquire(args.executable, args.acquire_output)
    if args.release_output:
        capture_release(args.executable, args.release_output)
