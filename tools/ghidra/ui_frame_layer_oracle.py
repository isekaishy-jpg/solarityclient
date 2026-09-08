"""Capture original 494AF0 layer/frame callback order from the pinned client.

Only graphics bucket begin/end and virtual region emission are intercepted.
The native loop selects dirty layers and eligible frames and orders callbacks.
"""
import argparse
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(mask, hidden):
    u = n.emulator()
    level, frames, vtable = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x4000
    n.write_words(u, level + 0xc, 0x100)
    n.write_words(u, level + 0x14, frames)
    n.write_words(u, level + 0x10c, mask)
    n.write_words(u, vtable + 0x84, n.STOP + 16)
    for i in range(4):
        frame = frames + i * 0x200
        n.write_words(u, frame, vtable)
        n.write_words(u, frame + 0xb4, 0x2000 if hidden & (1 << i) else 0)
        n.write_words(u, frame + 0x104, frame + 0x200 if i < 3 else 1)
    calls = []

    def hook(u, address, size, _):
        if address not in (0x485f00, 0x484450, n.STOP + 16):
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        arguments = 0
        if address == n.STOP + 16:
            layer = n.read_words(u, sp + 8, 1)[0]
            frame = (u.reg_read(UC_X86_REG_ECX) - frames) // 0x200
            calls.append((layer, frame))
            arguments = 2
        return_value(u, 0)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + 4 * arguments)

    u.hook_add(UC_HOOK_CODE, hook)
    u.reg_write(UC_X86_REG_ECX, level)
    n.invoke(u, 0x494af0, [])
    return calls


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# 494AF0: dirty layer mask, hidden frame mask, layer/frame callback pairs']
    for mask in range(32):
        for hidden in [0, 2, 5, 15]:
            calls = capture(mask, hidden)
            rows.append(' '.join(map(str, [mask, hidden, *[v for pair in calls for v in pair]])))
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native layer/frame sequences')
