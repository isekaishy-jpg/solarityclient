"""Capture original Unit_C monster path construction, including start adjustment.

Executes 0073C8E0 and all packet, packed-offset, projection and duration kernels.
The current transform and run speed are controlled inputs. Transport attachment
and the unrelated animation reset are intercepted; capture ends at the native
path-install or immediate-placement boundary. No path math is substituted.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from movement_path_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, vtable, buffer, payload_address, inputs, cvar = [native.HEAP + i * 0x4000 for i in range(6)]
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 0xd8, inputs)
    native.write_words(uc, vtable + 0x2c, native.STOP + 0x100, native.STOP + 0x100)
    native.write_words(uc, vtable + 0x38, native.STOP + 0x180)
    native.write_words(uc, 0xca1194, cvar)
    native.write_floats(uc, cvar + 0x2c, [4.0])
    # Virtual orientation accessor exposes the controlled caller input via x87.
    uc.mem_write(native.STOP + 0x180, b'\xd9\x05' + struct.pack('<I', inputs + 12) + b'\xc3')
    captured = []

    def intercept(u, address, _size, _data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in [0x6f0c70, 0x98b590]:
            pop = 16 if address == 0x6f0c70 else 4
            u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
            u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        elif address == native.STOP + 0x100:
            destination = native.read_words(u, sp + 4, 1)[0]
            u.mem_write(destination, bytes(u.mem_read(inputs, 12)))
            u.reg_write(UC_X86_REG_EAX, destination)
            u.reg_write(UC_X86_REG_ESP, sp + 8)
            u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        elif address == 0x6eb680:
            nodes, count, duration, flags, identity = native.read_words(u, sp + 4, 5)
            captured.append(('path', duration, flags, identity, native.read_words(u, nodes, count * 3)))
            u.reg_write(UC_X86_REG_EIP, native.STOP)
        elif address == 0x6f11b0:
            identity, destination, flags, _force = native.read_words(u, sp + 4, 4)
            captured.append(('place', 0, flags, identity, native.read_words(u, destination, 3)))
            u.reg_write(UC_X86_REG_EIP, native.STOP)

    uc.hook_add(UC_HOOK_CODE, intercept)
    rows = ['# original Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# current xyz/facing/run-speed | wire payload after GUID/control byte | install kind duration flags id controls']
    packets = []
    for flags in [0, 0x40000, 0xc0000, 0x2000]:
        for duration in [0, 100, 1000, 5001]:
            for points in [[(5., 0., 0.)], [(2., 0., 0.), (5., 2., 0.), (10., 0., 0.)], [(0.,0.,0.)]]:
                if flags & 0x80000 and len(points) < 2:
                    continue  # A cycle must supply its two closing controls.
                body = struct.pack('<3fIBII', 0., 0., 0., 37, 0, flags, duration)
                body += struct.pack('<I', len(points))
                if flags & 0x42000:
                    body += b''.join(struct.pack('<3f', *point) for point in points)
                else:
                    destination = points[-1]
                    body += struct.pack('<3f', *destination)
                    for point in points[:-1]:
                        offsets = [round((destination[i] * .5 - point[i]) * 4) for i in range(3)]
                        word = (offsets[0] & 0x7ff) | ((offsets[1] & 0x7ff) << 11) | ((offsets[2] & 0x3ff) << 22)
                        body += struct.pack('<I', word)
                packets.append(body)
    for destination in [(0.,0.,0.), (.01,0.,0.), (2.,0.,0.), (4.,0.,0.), (10.,0.,0.)]:
        packets.append(struct.pack('<3fIB', *destination, 37, 1))
    for current in [(-2., 0., 0.), (0., 0., 0.), (1., 1., 0.), (3., 0., 0.), (10., 0., 0.), (15., 2., 0.)]:
        for run_speed in [7., 15.]:
            native.write_floats(uc, inputs, [*current, .7])
            native.write_floats(uc, inputs + 0x94, [run_speed])
            for body in packets:
                uc.mem_write(payload_address, body)
                native.write_words(uc, buffer, 0, payload_address, 0, len(body), len(body), 0)
                captured.clear()
                uc.reg_write(UC_X86_REG_ECX, unit)
                invoke(uc, 0x73c8e0, [buffer, 0xdd, 0, 0, 0xff, 0])
                assert len(captured) == 1
                assert native.read_words(uc, buffer + 20, 1)[0] == len(body)
                kind, duration, flags, identity, nodes = captured[0]
                prefix = ' '.join(f'{bits(v):08x}' for v in [*current, .7, run_speed])
                values = ' '.join(f'{value:08x}' for value in [duration, flags, identity, *nodes])
                rows.append(f'{prefix} | {body.hex()} | {kind} {values}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} original monster path preparations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
