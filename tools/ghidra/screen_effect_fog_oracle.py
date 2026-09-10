"""Capture manual screen fog transitions with the original build-12340 code.

4F7020, 7ED870, 7ED820, 7ECD00 and 7F16F0 execute original instructions.
Model/UI/postprocess sinks are hooked and the WMO query reports no indoor bank.
The liquid and device-capability providers are explicit inputs. This captures
manual fog, restoration and latching, not full-screen shader or sound output.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('executable')
parser.add_argument('--output', required=True)
args = parser.parse_args()
n.initialize(args.executable)
u = n.emulator()
bank, rows, cvar, effect, vtable = [n.HEAP + i * 0x1000 for i in range(5)]
n.write_words(u, 0xad4518, 1)
n.write_words(u, 0xad4514, 5)
n.write_words(u, 0xad4528, bank)
n.write_words(u, bank, *[rows + i * 40 for i in range(5)])
for i, kind in enumerate([0, 1, 2, 3, 99]):
    n.write_words(u, rows + i * 40, i + 1, 0, kind, 0, 0, 0, 0, 0xffffffff, 0, 0)
n.write_words(u, 0xd45774, cvar)
n.write_words(u, 0xb7435c, effect, effect + 16, effect + 32, effect + 48)
n.write_words(u, effect, vtable)
n.write_words(u, vtable + 16, n.STOP + 16)
n.write_words(u, 0xd38184, 0)
supported, liquid = 0, 0

def hook(u, address, size, context):
    if address in (0x8c02e0, 0x4c8fa0, 0x7e7fe0, 0x77fb90):
        return_value(u, 0)
    elif address == 0x682d20:
        return_value(u, supported)
    elif address == 0x780620:
        return_value(u, liquid)
    elif address == n.STOP + 16:
        return_value(u, 0)
        u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + 8)

u.hook_add(UC_HOOK_CODE, hook)
records = ['# fog clip power liquid supported ffx step ID(-1=no callback) color start end exponent sky manual; step zero starts a fresh sequence.']
sequence = [0, 3, 5, 1, 3, 3, 2, 3, 4, 3, 0]
for clip, power, liquid, supported, ffx in itertools.product(
        [100., 177., 200., 245., 300., 500., 700., 777., 1277.], [0, 1], [0, 1], [0, 1], [0, 1]):
    n.write_floats(u, 0xd38b40, [clip])
    n.write_words(u, 0xd38acc, power, 0)
    n.write_words(u, cvar + 0x30, ffx)
    n.write_words(u, 0xd38ccc, 1)
    n.write_words(u, 0xd38bf4, 0xff123456)
    n.write_floats(u, 0xd38c1c, [300., .45, 2.5])
    for step, id in enumerate(sequence):
        n.invoke(u, 0x4f7020, [id])
        n.invoke(u, 0x7f16f0, [])
        words = [*n.read_words(u, 0xd38b8c, 4), n.read_words(u, 0xd38ccc, 1)[0], n.read_words(u, 0xd38ad0, 1)[0]]
        records.append('fog ' + ' '.join(map(str, [clip, power, liquid, supported, ffx, step, id]))
                       + ' ' + ' '.join(f'{word:08x}' for word in words))
# Changing ffx/farclip between callbacks must preserve the latched manual curve/color.
for power, liquid in itertools.product([0, 1], [0, 1]):
    supported = 1
    n.write_words(u, 0xd38acc, power, 0)
    n.write_words(u, 0xd38ccc, 1)
    n.write_words(u, 0xd38bf4, 0xff123456)
    n.write_floats(u, 0xd38c1c, [300., .45, 2.5])
    for step, (clip, ffx, id) in enumerate([
            (777., 0, 3), (300., 1, -1), (100., 1, -1), (500., 1, 3),
            (777., 0, -1), (177., 0, 5), (777., 0, 3), (777., 1, 0)]):
        n.write_floats(u, 0xd38b40, [clip])
        n.write_words(u, cvar + 0x30, ffx)
        if id >= 0:
            n.invoke(u, 0x4f7020, [id])
        n.invoke(u, 0x7f16f0, [])
        words = [*n.read_words(u, 0xd38b8c, 4), n.read_words(u, 0xd38ccc, 1)[0], n.read_words(u, 0xd38ad0, 1)[0]]
        records.append('fog ' + ' '.join(map(str, [clip, power, liquid, supported, ffx, step, id]))
                       + ' ' + ' '.join(f'{word:08x}' for word in words))
Path(args.output).write_text('\n'.join(records) + '\n', encoding='utf-8')
print(f'Captured {len(records)-1} manual fog cases')
