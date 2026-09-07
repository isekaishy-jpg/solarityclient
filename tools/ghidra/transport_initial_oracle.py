"""Capture original GameObject facing for initial transport map handles.

Runs 70C310, quaternion decoding/composition, 4F4630 and 4F42A0 unchanged.
The two parent-GUID providers return a controlled resident parent's quaternion
and facing. No client process, OS entry point, allocation, or map loader runs.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_path_oracle import invoke
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, poses, output):
    native.initialize(executable)
    uc = native.emulator()
    owner, behavior, quaternion, facing, result = [native.HEAP + i * 0x1000 for i in range(5)]
    native.write_words(uc, behavior + 4, owner)
    # Cdecl resident-parent quaternion provider: copy four already decoded words.
    provider = b'\x8b\x44\x24\x0c'
    for offset in range(0, 16, 4):
        provider += b'\x8b\x15' + struct.pack('<I', quaternion + offset)
        provider += b'\x89\x50' + bytes([offset])
    uc.mem_write(0x74b510, provider + b'\xc3')
    # The parent's virtual facing returns the f32 produced by native 70C310.
    uc.mem_write(0x74b590, b'\xd9\x05' + struct.pack('<I', facing) + b'\xc3')
    uc.mem_write(native.STOP + 16, b'\xd9\x1d' + struct.pack('<I', result))
    packed = [int(line.split()[6], 16) for line in Path(poses).read_text().splitlines()
              if line and not line.startswith('#')]
    cases = [(value, None) for value in packed]
    cases += [(packed[i + 32], packed[i + 64]) for i in range(128)]
    lines = ['# Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# child packed, parent packed or -, initial map facing IEEE754 hex']
    for child, parent in cases:
        native.write_words(uc, owner + 0xd8 + 8, 0, 0)
        if parent is not None:
            uc.mem_write(owner + 0xf8, struct.pack('<Q', parent))
            uc.reg_write(UC_X86_REG_ECX, behavior)
            invoke(uc, 0x70c310, [])
            uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
            uc.mem_write(facing, bytes(uc.mem_read(result, 4)))
            uc.reg_write(UC_X86_REG_ECX, quaternion)
            invoke(uc, 0x982340, [owner + 0xf8])
            native.write_words(uc, owner + 0xd8 + 8, 2, 0)
        uc.mem_write(owner + 0xf8, struct.pack('<Q', child))
        uc.reg_write(UC_X86_REG_ECX, behavior)
        invoke(uc, 0x70c310, [])
        uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
        parent_word = '-' if parent is None else f'{parent:016x}'
        lines.append(f'{child:016x} {parent_word} {native.read_words(uc, result, 1)[0]:08x}')
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'captured {len(cases)} original initial map-model facings')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('poses')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.poses, args.output)
