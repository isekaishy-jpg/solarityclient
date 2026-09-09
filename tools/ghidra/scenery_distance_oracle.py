"""Execute pinned 12340 scenery classification, thresholds, and admission.

Only 823F10's model activation side effect is replaced. 7BDB10, its bounds
math, 78F570, and 791CB0 execute original instructions. No client entry point
or OS code runs. Requires a locally owned pinned Wow.exe and Unicorn.
"""
import argparse
import math
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as native


def capture(executable, output):
    """Write float rows consumed by the isolated runtime regression test."""
    native.initialize(executable)
    rows = ["# min[3] max[3] transform[16] camera[3] detail opacity; build 12340"]
    uc = native.emulator()
    entity, model, shared, header = [native.HEAP + x for x in (0, 0x1000, 0x2000, 0x3000)]
    native.write_words(uc, entity + 0x34, model)
    native.write_words(uc, model + 0x2c, shared)
    native.write_words(uc, shared + 8, 1)
    native.write_words(uc, shared + 0x150, header)
    native.write_floats(uc, entity + 0x78, [1])
    native.write_words(uc, 0xcd774c, 0x4001)

    def activate(uc, address, size, user):
        stack = uc.reg_read(UC_X86_REG_ESP)
        target = native.read_words(uc, stack, 1)[0]
        uc.reg_write(UC_X86_REG_ESP, stack + 8)
        uc.reg_write(UC_X86_REG_EIP, target)

    uc.hook_add(UC_HOOK_CODE, activate, begin=0x823f10, end=0x823f10)
    for extent in (0.5, 1, 1.0001, 4, 4.0001, 15, 15.0001, 100, 100.0001):
        for angle, translation in ((0, [0, 0, 0]), (0.61, [1100, -4290, 20])):
            cosine, sine = math.cos(angle), math.sin(angle)
            transform = [cosine, sine, 0, 0, -sine, cosine, 0, 0, 0, 0, 1, 0, *translation, 1]
            bounds = [-extent / 2, -0.1, -0.1, extent / 2, 0.1, 0.1]
            native.write_floats(uc, header + 0xa0, [*bounds, extent])
            native.write_floats(uc, entity + 0xd8, transform)
            native.invoke(uc, 0x7bdb10, [entity])
            category = uc.mem_read(entity + 0x24, 1)[0]
            center = native.read_floats(uc, entity + 0x38, 3)
            for detail in (0.5, 1, 1.5):
                native.invoke(uc, 0x78f570, [struct.unpack('<I', struct.pack('<f', detail))[0]])
                far = native.read_floats(uc, 0xadf3a0 + 4 * category, 1)[0]
                fade = native.read_floats(uc, 0xadf38c + 4 * category, 1)[0]
                for fraction in (-0.1, 0, 0.009, 0.011, 0.5, 0.989, 0.991, 1, 1.01):
                    camera = [center[0] + far - fade + fade * fraction, center[1], center[2]]
                    native.write_floats(uc, 0xcd8f5c, camera)
                    native.write_floats(uc, model + 0x178, [0])
                    native.invoke(uc, 0x791cb0, [entity])
                    opacity = native.read_floats(uc, model + 0x178, 1)[0]
                    rows.append(" ".join(map(str, [*bounds, *transform, *camera, detail, opacity])))
    Path(output).write_text("\n".join(rows) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("executable")
    parser.add_argument("output")
    args = parser.parse_args()
    capture(args.executable, args.output)
