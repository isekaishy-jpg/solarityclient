"""Execute native invisibility noise, frame advancement and mesh publication.

The pinned 7E8990 constructor owns the original seeded random stream; 7E9B10
advances its banks and camera-relative angle. Device/resource boundaries are
supplied. 8C0740 and 8C0DE0 generate and quantize the 6x6 mesh unchanged.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_EBP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


class Producer:
    def __init__(self):
        self.u = n.emulator()
        self.owner, self.scene, self.blur, self.output, self.device, self.table, self.buffer, self.vertices = [
            n.HEAP + i * 0x4000 for i in range(8)]
        self.combine = self.owner + 0x1000
        self.mode = 'constructor'
        n.write_words(self.u, 0xc5df88, self.device)
        n.write_words(self.u, self.device, self.table)
        for offset, address in [(0x110, n.STOP + 0x100), (0xd8, n.STOP + 0x200),
                                (0xdc, n.STOP + 0x300), (0x118, n.STOP + 0x400)]:
            n.write_words(self.u, self.table + offset, address)
        self.u.hook_add(UC_HOOK_CODE, self.hook)
        self.u.reg_write(UC_X86_REG_ECX, self.owner)
        n.invoke(self.u, 0x7e8990, [0, 0, 0, 0, 0])
        n.write_words(self.u, self.owner + 4, self.scene)
        n.write_words(self.u, self.owner + 0x24, self.blur)
        self.initial = bytes(self.u.mem_read(self.owner + 0x34, 108 * 4))
        n.write_words(self.u, self.combine + 4, self.scene, self.blur)
        n.write_words(self.u, self.combine + 0x24, self.output)

    def hook(self, u, address, size, context):
        sp = u.reg_read(UC_X86_REG_ESP)
        cleanup = None
        if address in (0x8c0300, n.STOP + 0x100):
            cleanup = 20
        elif address == 0x8c1890:
            if self.mode == 'advance':
                bp = u.reg_read(UC_X86_REG_EBP)
                self.angle = n.read_floats(u, bp - 0x10, 1)[0]
                u.reg_write(UC_X86_REG_EIP, n.STOP)
                u.emu_stop()
            else:
                cleanup = 0
        elif address == 0x4b6cb0:
            return_value(u, 1)
        elif address == 0x685f50:
            cleanup = 8
        elif address == n.STOP + 0x400:
            cleanup = 16
        elif address == 0x682400:
            u.reg_write(UC_X86_REG_EIP, n.STOP)
            u.emu_stop()
        elif address == 0x682d20:
            return_value(u, 1)
        elif address == 0x684850:
            return_value(u, self.buffer)
            u.reg_write(UC_X86_REG_ESP, sp + 16)
        elif address == n.STOP + 0x200:
            return_value(u, self.vertices)
            u.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == n.STOP + 0x300:
            cleanup = 8
        elif address == 0x6844c0:
            cleanup = 12
        if cleanup is not None:
            return_value(u, 0)
            u.reg_write(UC_X86_REG_ESP, sp + 4 + cleanup)

    def advance(self, delta, view):
        self.mode = 'advance'
        n.write_floats(self.u, 0xcd76a0, [delta])
        # 8C0290 returns the saved GX view, captured before FFX resets it.
        n.write_floats(self.u, 0xb24ae0, view)
        self.u.reg_write(UC_X86_REG_ECX, self.owner)
        n.invoke(self.u, 0x7e9b10, [])
        pointers = n.read_words(self.u, self.owner + 0x274, 3)
        return {
            'angle': self.angle,
            'time': n.read_floats(self.u, 0xd38150, 1)[0],
            'phase': n.read_floats(self.u, self.owner + 0x280, 1)[0],
            'random': n.read_words(self.u, self.owner + 0x284, 2),
            'banks': [list(n.read_floats(self.u, p, 36)) for p in pointers],
            'noise': n.read_floats(self.u, self.owner + 0x1e4, 36),
        }

    def mesh(self, width, height):
        for address, w, h in [(self.scene, width, height), (self.blur, width // 4, height // 4)]:
            n.write_words(self.u, address, 1, w, h, w, h)
        n.invoke(self.u, 0x8c0740, [self.blur + 12, self.scene + 12, self.scene + 4, 0xd45848, 0xd459f8, 6, 0])
        n.invoke(self.u, 0x8c0de0, [36, 0xd45848, 0xd459f8, self.owner + 0x1e4])
        return bytes(self.u.mem_read(self.vertices, 36 * 24))

    def fade(self, delta, reset=False):
        self.mode = 'combine'
        if reset:
            bank = self.combine + 0x100
            n.write_words(self.u, bank, 0, self.combine)
            n.write_words(self.u, self.output, 0xa418ec)
            n.write_words(self.u, self.output + 12, 2, bank)
            n.write_words(self.u, 0xd45780, self.output)
            # Selecting even the same owner invokes its outgoing callback.
            n.invoke(self.u, 0x8c02e0, [self.output])
        for address, w, h in [(self.scene, 64, 64), (self.blur, 16, 16), (self.output, 64, 64)]:
            n.write_words(self.u, address, 1, w, h, w, h)
        n.write_floats(self.u, 0xcd76a0, [delta])
        self.u.reg_write(UC_X86_REG_ECX, self.combine)
        n.invoke(self.u, 0x7e8c80, [])
        return n.read_floats(self.u, self.combine + 0x3c, 1)[0]


def capture(output):
    producer = Producer()
    records = ['# delta reset axis[3] time phase angle fade colors[36] banks[108]; hex words']
    axes = [(1., 0., 0.), (0., 1., 0.), (-1., 0., 0.), (0., -1., 0.),
            (.3, -.8, .5), (.3, -.8, -.5), (.3, .8, .0001), (.3, .8, -.00010001)]
    deltas = [0., 1/60, .5, 0., .2, 1/60, 2., 0., .75, 4., 1/1200, 1., 0., 0.]
    for index in range(168):
        delta = deltas[index % len(deltas)]
        axis = axes[index % len(axes)]
        reset = index in [0, 15, 63, 101, 150]
        view = [*axis, 0., 0., 1., 0., 0., 0., 0., 1., 0.]
        state = producer.advance(delta, view)
        mesh = producer.mesh(64, 64)
        fade = producer.fade(delta, reset)
        floats = [delta, *axis, state['time'], state['phase'], state['angle'], fade]
        bits = list(struct.unpack('<8I', struct.pack('<8f', *floats)))
        words = [bits[0], int(reset), *bits[1:]]
        words += [mesh[i * 24 + 12] for i in range(36)]
        words += list(struct.unpack('<108I', struct.pack('<108f', *sum(state['banks'], []))))
        records.append(' '.join(f'{value:08x}' for value in words))
    output.write_text('\n'.join(records) + '\n', encoding='utf-8')
    metadata = {
        'executable_sha256': hashlib.sha256(n.data).hexdigest(),
        'initial_banks_sha256': hashlib.sha256(producer.initial).hexdigest(),
        'fixture_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
        'frames': len(records) - 1,
        'native_functions': ['7E8990', '4C1510', '464580', '7E9B10', '8C0740', '8C0DE0', '7E8C80', '8C02E0', '7E8E20'],
    }
    output.with_suffix('.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(metadata))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    capture(args.output)
