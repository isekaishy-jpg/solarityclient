"""Execute native player screen-effect selection and global-palette replacement.

The harness supplies local-player/Spell lookup and sampled palette words.
4F88B0, 7EB180 and 7F3230's override block execute original instructions.
"""
import argparse
import itertools
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def selection():
    u = n.emulator()
    player, fields, aura_data = [n.HEAP + i * 0x4000 for i in range(3)]
    present = True
    selected = []
    spells = {1: ([260, 260, 260], [141, 242, 0]),
              2: ([0, 260, 260], [999, 242, 141]),
              3: ([0, 0, 260], [999, 999, 0]),
              4: ([0, 0, 0], [141, 242, 141]),
              5: ([0, 0, 260], [0, 0, 0xffffffff])}

    def ret(value=0, arguments=0):
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + arguments * 4)

    def hook(u, address, size, context):
        stack = u.reg_read(UC_X86_REG_ESP)
        if address in (0x4d3790, 0x8b7da0, 0x5eeb70):
            ret()
        elif address == 0x4d4db0:
            ret(player if present else 0)
        elif address == 0x4cfd20:
            spell, record = n.read_words(u, stack + 4, 2)
            if spell in spells:
                aura, misc = spells[spell]
                n.write_words(u, record + 0x17c, *aura)
                n.write_words(u, record + 0x1b8, *misc)
            ret(int(spell in spells), 2)
        elif address == 0x4f7020:
            selected.append(n.read_words(u, stack + 4, 1)[0])
            ret()

    u.hook_add(UC_HOOK_CODE, hook)
    n.write_words(u, player + 0x1008, fields)
    cases = [[], [1], [2], [3], [4], [5], [999], [1, 2], [2, 1],
             [1, 0, 4], [1, 999], [2, 3], [3, 2], [0, 0, 5]]
    rows = ['# 4F88B0: present external ghost invisible arena slot-count spells... selected-ID.']
    for present, external, ghost, invisible, arena, slots in itertools.product(
            [False, True], [False, True], [False, True], [False, True], [False, True], cases):
        selected.clear()
        n.write_words(u, player + 0xdd0, 0xffffffff if external else len(slots))
        n.write_words(u, player + 0xc54, len(slots), aura_data)
        n.write_words(u, fields + 8, 16 if ghost else 0)
        n.write_words(u, fields + 0x10e4, 0x40000000 if invisible else 0)
        n.write_words(u, 0xbea570, 4 if arena else 0)
        for index, spell in enumerate(slots):
            n.write_words(u, (aura_data if external else player + 0xc50) + index * 24 + 8, spell)
        n.invoke(u, 0x4f88b0, [])
        assert len(selected) == 1
        rows.append('select ' + ' '.join(map(str, [int(present), int(external), int(ghost),
            int(invisible), int(arena), len(slots), *slots, selected[0]])))
    return rows


def palette():
    u = n.emulator()
    light, bank, parameters, parameter, skyboxes, skybox = [n.HEAP + i * 0x1000 for i in range(6)]
    frame = n.STACK + 0x10000
    requests = []
    n.write_words(u, bank, light)
    n.write_words(u, 0xaf4a14, 1)
    n.write_words(u, 0xaf4a10, 2)
    n.write_words(u, 0xaf4a24, parameters)
    n.write_words(u, parameters, parameter, 0)
    n.write_words(u, light + 0x1c, 1, 0, 2, 99, 1, 1, 1, 1)
    n.write_words(u, parameter + 8, 9)
    n.write_words(u, 0xaf49a8, 9)
    n.write_words(u, 0xaf49a4, 9)
    n.write_words(u, 0xaf49b8, skyboxes)
    n.write_words(u, skyboxes, skybox)
    n.write_words(u, skybox, 9, 123, 3)

    def hook(u, address, size, context):
        if address == 0x7f3336:
            u.emu_stop()
        elif address == 0x7ecd80:
            n.write_words(u, u.reg_read(UC_X86_REG_ESI), *[0x40000000 + i for i in range(39)])
            return_value(u, 0)
        elif address == 0x7f30c0:
            requests.append(u.reg_read(UC_X86_REG_EDI))
            return_value(u, 999)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# 7F3230 global override: condition model weight request-count retained 39 palette words.']
    for condition in [0xffffffff, *range(8)]:
        requests.clear()
        n.write_words(u, 0xd38b58, condition, 0, 0)
        n.write_words(u, frame - 0xb0, *[0x3f000000 + i for i in range(39)])
        n.write_words(u, 0xd38bd4, *([0] * 39))
        u.reg_write(UC_X86_REG_EBP, frame)
        u.reg_write(UC_X86_REG_ESP, frame - 0x200)
        u.reg_write(UC_X86_REG_EDI, bank)
        u.emu_start(0x7f345a, n.STOP, count=10000)
        model, weight = n.read_words(u, 0xd38b5c, 2)
        values = n.read_words(u, 0xd38bd4, 39)
        rows.append(f'palette {condition:08x} {model} {weight:08x} {len(requests)} '
                    + ' '.join(f'{v:08x}' for v in values))
    return rows


def draw():
    u = n.emulator()
    device, window = n.HEAP, n.HEAP + 0x4000
    models = [n.HEAP + 0x8000 + i * 0x100 for i in range(4)]
    defaults, phases, draws, advances = [], [], [], []
    n.write_words(u, 0xc5df88, device)
    n.write_floats(u, device + 0xf70, [0., 1., 0., 1., 0., 1.])
    n.write_floats(u, window, [0., 0., 1., 1.])
    n.write_words(u, 0xd38ccc, 1)
    n.write_words(u, 0xd38ad4, 1)
    for model in models[:3]:
        n.write_words(u, model + 0x18, 1)

    def hook(u, address, size, context):
        stack = u.reg_read(UC_X86_REG_ESP)
        if address in (0x681f60, 0x682e70, 0x76c360):
            return_value(u, 0)
        elif address == 0x824fc0:
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + 8)
        elif address == 0x86ae20:
            return_value(u, 12345)
        elif address in (0x9abd50, 0x9ac660, 0x9acb00, 0x9acd40):
            defaults.append(address)
            return_value(u, 0)
        elif address == 0x81c9c0:
            advances.append(n.read_words(u, stack + 4, 1)[0])
            return_value(u, 0)
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + 4)
        elif address == 0x7ecf20:
            phases.append(n.read_words(u, stack + 4, 1)[0])
            return_value(u, 0)
        elif address == 0x7f08c0:
            draws.append(n.read_words(u, stack + 4, 2))
            return_value(u, 0)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# 7F09B0: global-present ready global-weight ordinary-flags default calls advance-count phase-count draw-call-count model-index/weight pairs.']
    weights = [-1., 0., .5, .99, .99000006, 1., 2.]
    import struct
    bits = lambda v: struct.unpack('<I', struct.pack('<f', v))[0]
    for present, ready, weight, flags in itertools.product([0, 1], [0, 1], weights, [0, 2]):
        defaults.clear(); phases.clear(); draws.clear(); advances.clear()
        n.write_words(u, 0xd38b5c, models[3] if present else 0, bits(weight))
        n.write_words(u, models[3] + 0x18, ready)
        n.write_words(u, 0xd38b64, *models[:3], bits(1.), bits(.3), bits(.4), flags, 2, 2)
        n.invoke(u, 0x7f09b0, [window])
        assert phases == [*models[:3], models[3] if present else 0]
        row = f'draw {present} {ready} {bits(weight):08x} {flags} {len(defaults)} {len(advances)} {len(phases)} {len(draws)}'
        for model, opacity in draws:
            row += f' {models.index(model)} {opacity:08x}'
        rows.append(row)
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('--output', required=True)
    parser.add_argument('--light-output', required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    Path(args.output).write_text('\n'.join(selection() + draw()) + '\n')
    Path(args.light_output).write_text('\n'.join(palette()) + '\n')
