"""Capture build-12340 wound routing and transient secondary timer setup.

736640 runs with its original posture, movement, AnimationData and template
gates. Supplied boundaries are the already-tested dead predicate, resident
model queries, vehicle control, and the final model request. Tier resolution
runs its native non-tiered path. Timer cases execute 826DD0 and 826B00 with
only the CRT roll supplied. No client entry point or OS services run.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EBP,
                              UC_X86_REG_ESI, UC_X86_REG_EIP, UC_X86_REG_ESP)

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture_routes():
    uc = native.emulator()
    unit, fields, movement, model, template, effect, handler, bank, records = [
        native.HEAP + x * 0x2000 for x in range(9)
    ]
    # 73F660 binds Unit_C +D8 to its embedded Movement_C at +788.
    movement = unit + 0x788
    native.write_words(uc, unit + 0xd0, fields, 0, movement)
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 505)
    native.write_words(uc, 0xad30e8, bank)
    for animation in range(506):
        record = records + animation * 32
        native.write_words(uc, bank + animation * 4, record)
        native.write_words(uc, record + 24, animation)
    case, calls = {}, []

    def ret(value=0, pop=0):
        sp = uc.reg_read(UC_X86_REG_ESP)
        return_value(uc, value & 0xffffffff)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def provider(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x71f560:
            ret(case['dead'])
        elif address == 0x824f00:
            ret(case['loaded'], 8)
        elif address == 0x8267e0:
            layer = native.read_words(uc, sp + 4, 1)[0]
            ret(case['primary'] if layer == 0xffffffff else case['current'], 4)
        elif address == 0x825ee0:
            ret(case['available'], 4)
        elif address == 0x7571c0:
            ret(case['vehicle'])
        elif address == 0x735820:
            calls.append(native.read_words(uc, sp + 4, 9)[1:])
            ret(0, 36)

    uc.hook_add(UC_HOOK_CODE, provider)
    defaults = dict(critical=0, attack=0, flags=0, stand=0, mounted=0,
                    primary=0, current=-1, layer=4, dead=0, loaded=1,
                    available=1, present=1, template=-1, effect=0,
                    state=0, state2=0, handler=0, vehicle=0)
    variants = [dict()]
    for key, values in dict(
        flags=[1, 2, 4, 8, 0x10, 0x1000, 0x200000, 0x400000, 0x800000,
               0x2000000, 0x1000000], stand=list(range(1, 10)), mounted=[1],
        attack=[1, 0x100000000], critical=[1], primary=[25, 26, 27, 28, 29, 98, 121],
        current=[98, 121], layer=[6, -1], dead=[1], loaded=[0], available=[0],
        present=[0], template=[0, 8], effect=[4], state=[0x400, 0x40000000],
        state2=[4], handler=[0x200, 0x2000, 0x2400], vehicle=[1],
    ).items():
        for value in values:
            variants.append({key: value, 'primary': value if key == 'primary' else 42})
    for primary in [0, 4, 25, 29, 30, 98]:
        for layer in [4, 6, -1]:
            variants.append(dict(primary=primary, flags=0x200000, layer=layer))
    rows = ['# ' + ' '.join(defaults) + ' selectedAnimation selectedLayer secondary blend']
    for changes in variants:
        case = defaults | changes
        native.write_words(uc, unit + 0xb4, model if case['present'] else 0)
        native.write_words(uc, unit + 0xb84, case['layer'] & 0xffffffff)
        native.write_words(uc, unit + 0xa20, case['attack'] & 0xffffffff, case['attack'] >> 32)
        native.write_words(uc, unit + 0x98c, model if case['mounted'] else 0)
        native.write_words(uc, movement + 0x44, case['flags'])
        native.write_words(uc, fields + 0x110, case['stand'])
        native.write_words(uc, unit + 0x964, template if case['template'] != -1 else 0)
        native.write_words(uc, template + 12, case['template'] & 0xffffffff)
        native.write_words(uc, unit + 0xa8, effect)
        native.write_words(uc, effect + 0x48, case['effect'])
        native.write_words(uc, unit + 0x7cc, case['flags'] | case['state'], case['state2'])
        native.write_words(uc, unit + 0x844, handler)
        native.write_words(uc, handler + 0x20, case['handler'])
        native.write_words(uc, unit + 0xf5c, template if case['vehicle'] else 0)
        calls.clear()
        uc.reg_write(UC_X86_REG_ECX, unit)
        try:
            native.invoke(uc, 0x736640, [case['critical']])
        except Exception:
            print(case, hex(uc.reg_read(UC_X86_REG_EIP)))
            raise
        assert len(calls) <= 1
        if calls:
            layer, animation, variation, offset, speed, blend, primary, propagate = calls[0]
            assert (variation, offset, speed, propagate) == (0xffffffff, 0, 0x3f800000, 0)
            output = [animation, layer if layer < 0x80000000 else layer - 0x100000000, 1-primary, blend]
        else:
            output = [-1, -2, 0, 0]
        rows.append(' '.join(map(str, [*case.values(), *output])))
    return rows


def capture_timers():
    uc = native.emulator()
    model, resource, data, scene, sequence, bone = [native.HEAP + i * 0x1000 for i in range(6)]
    native.write_words(uc, model + 0x28, scene, resource)
    native.write_words(uc, model + 0x94, bone)
    native.write_words(uc, resource + 0x150, data)
    native.write_words(uc, data + 0x20, sequence)

    def provider(uc, address, size, context):
        if address == 0x88b867:
            return_value(uc, 12345)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = ['# duration scene phase start end blendEnd inverseDuration amplitude']
    for duration in [0, 1, 150, 800, 1234]:
        for now in [1000, 0xfffffff0]:
            for phase in [0, 4]:
                native.write_words(uc, scene + 12, now)
                native.write_words(uc, scene + 28, phase)
                native.write_words(uc, sequence + 4, duration)
                native.write_words(uc, sequence + 20, 1, 1)
                uc.reg_write(UC_X86_REG_ECX, model)
                native.invoke(uc, 0x826dd0, [0, 0, 0, 0, 0x3f800000])
                start, end = native.read_words(uc, bone + 0x70, 2)
                deadline, inverse, amplitude = native.read_words(uc, bone + 0x9c, 3)
                rows.append(f'{duration} {now} {phase} {start} {end} {deadline} {inverse:08x} {amplitude:08x}')
    return rows


def capture_bone_clocks():
    """Run complete clock inheritance/sampling block 82F426..82F783.

    Four parent-first bones are root, leg, spine and head. Bone 3 is a spine
    descendant; the leg is not. There are no authored matrices or callbacks.
    """
    uc = native.emulator()
    model, resource, data, scene, sequences, bones, descriptors = [native.HEAP + i * 0x1000 for i in range(7)]
    frame = native.STACK + 0x10000
    native.write_words(uc, model + 0x28, scene, resource)
    native.write_words(uc, model + 0x94, bones)
    native.write_words(uc, resource + 0x150, data)
    native.write_words(uc, data + 0x20, sequences)
    native.write_words(uc, data + 0x2c, 4, descriptors)
    native.write_words(uc, frame - 4, data)
    # The surrounding sampler retains ST0=1 and ST1=0 throughout this block.
    uc.mem_write(native.STOP + 16, b'\xdb\xe3\xd9\xee\xd9\xe8')

    def provider(uc, address, size, context):
        if address == 0x88b867:
            return_value(uc, 12345)

    uc.hook_add(UC_HOOK_CODE, provider)
    for index, duration in enumerate([2000, 800]):
        native.write_words(uc, sequences + index * 64 + 4, duration)
        native.write_words(uc, sequences + index * 64 + 20, 1, 1)
    for index, parent in enumerate([0xffff, 0, 0, 2]):
        uc.mem_write(descriptors + index * 88 + 8, struct.pack('<H', parent))
    rows = ['# woundBone scene elapsed bone primarySequence primaryTime secondarySequence secondaryTime weight']
    for wound_bone in [0, 2]:
        for now in [1000, 0xfffffff0]:
            for elapsed in [0, 1, 200, 400, 799, 800, 801]:
                uc.mem_write(bones, bytes(4 * 172))
                for index in range(4):
                    uc.mem_write(bones + index * 172 + 0x48, b'\xff\xff')
                    uc.mem_write(bones + index * 172 + 0x6c, b'\xff\xff')
                native.write_words(uc, scene + 12, (now - 100) & 0xffffffff)
                uc.reg_write(UC_X86_REG_ECX, model)
                native.invoke(uc, 0x826b00, [0, 0, 0, 0x3f800000, bones + 0x40])
                native.write_words(uc, scene + 12, now)
                uc.reg_write(UC_X86_REG_ECX, model)
                native.invoke(uc, 0x826dd0, [1, wound_bone, 0, 0, 0x3f800000])
                native.write_words(uc, scene + 12, (now + elapsed) & 0xffffffff)
                for index in range(4):
                    uc.emu_start(native.STOP + 16, native.STOP + 22, count=3)
                    for reg, value in [(UC_X86_REG_EBP, frame), (UC_X86_REG_ESP, frame - 0x1000),
                                       (UC_X86_REG_ESI, model), (UC_X86_REG_EDX, 4)]:
                        uc.reg_write(reg, value)
                    native.write_words(uc, frame + 24, index)
                    uc.emu_start(0x82f426, 0x82f783, count=1000)
                    assert uc.reg_read(UC_X86_REG_EIP) == 0x82f783
                    bone = bones + index * 172
                    primary_time, primary = native.read_words(uc, bone + 0x40, 2)
                    secondary_time, secondary = native.read_words(uc, bone + 0x64, 2)
                    weight = native.read_words(uc, bone + 0xa8, 1)[0]
                    rows.append(f'{wound_bone} {now} {elapsed} {index} {primary & 65535} {primary_time} {secondary & 65535} {secondary_time} {weight:08x}')
    return rows


def capture_attack_owner():
    """Run 756800, 754FF0, 98E520 and 756770 with real packet readers.

    Only object lookup and UI dispatch are supplied. No active swing/spell
    records are installed, so 756180 has no additional retirement work.
    """
    uc = native.emulator()
    unit, descriptor, packet, payload = [native.HEAP + i * 0x2000 for i in range(4)]
    native.write_words(uc, unit + 8, descriptor)
    native.write_words(uc, descriptor, 7, 0, 0x19)
    events = []

    def provider(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d4db0:
            low, high = native.read_words(uc, sp + 4, 2)
            return_value(uc, unit if low == 7 and high == 0 else 0)
        elif address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x81b530:
            events.append(native.read_words(uc, sp + 4, 1)[0])
            return_value(uc, 0)
        elif address == 0x5206e0:
            return_value(uc, 0)

    def packed(guid):
        raw = guid.to_bytes(8, 'little')
        return bytes([sum(1 << index for index, value in enumerate(raw) if value)]) + bytes(value for value in raw if value)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = ['# opcode body previousTarget retainedTarget event']
    for opcode, attacker, target, previous in [
        (0x143, 7, 0xf130123456789abc, 0), (0x143, 7, 0, 8),
        (0x143, 8, 99, 8), (0x144, 7, 0xf130123456789abc, 99),
        (0x144, 7, 0, 99), (0x261, 7, 0, 99), (0x144, 8, 99, 99),
    ]:
        body = struct.pack('<QQ', attacker, target) if opcode == 0x143 else packed(attacker) + packed(target) + bytes(4)
        uc.mem_write(payload, body)
        native.write_words(uc, packet, 0, payload, 0, len(body), len(body), 0)
        native.write_words(uc, unit + 0xa20, previous & 0xffffffff, previous >> 32)
        events.clear()
        native.invoke(uc, 0x756800, [0, opcode, 0, packet])
        low, high = native.read_words(uc, unit + 0xa20, 2)
        rows.append(f'{opcode:04x} {body.hex()} {previous:016x} {low + (high << 32):016x} {events[0] if events else -1}')
    return rows


def capture_death_clear():
    """Run 73917C..739500 with original death predicates and layer clear APIs."""
    uc = native.emulator()
    unit, model, resource, data, scene, bones, descriptors, lookup, bank, records = [
        native.HEAP + i * 0x2000 for i in range(10)
    ]
    frame = native.STACK + 0x10000
    native.write_words(uc, unit + 0xb4, model)
    native.write_words(uc, model + 0x10, 1)
    native.write_words(uc, model + 0x28, scene, resource)
    native.write_words(uc, model + 0x94, bones)
    native.write_words(uc, resource + 0x150, data)
    native.write_words(uc, data + 0x2c, 4, descriptors, 7, lookup)
    native.write_words(uc, scene + 12, 1000)
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 505)
    native.write_words(uc, 0xad30e8, bank)
    for animation in range(506):
        record = records + animation * 32
        native.write_words(uc, bank + animation * 4, record)
        native.write_words(uc, record + 24, animation)
    for index, parent in enumerate([0xffff, 0, 0, 2]):
        uc.mem_write(descriptors + index * 88 + 8, struct.pack('<H', parent))

    def provider(uc, address, size, context):
        if address == 0x824f00:
            sp = uc.reg_read(UC_X86_REG_ESP)
            return_value(uc, 1)
            uc.reg_write(UC_X86_REG_ESP, sp + 12)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = ['# animation key mappedBone rootSecondary spineSecondary']
    for animation in [0, 8, 1, 6, 131, 132, 466, 467, 468, 472]:
        for key, mapped in [(4, 2), (6, 2), (4, 0), (-1, 0)]:
            uc.mem_write(bones, bytes(4 * 172))
            for index in range(4):
                for offset in [0x48, 0x6c, 0x96]:
                    uc.mem_write(bones + index * 172 + offset, b'\xff\xff')
            for index in [0, 2]:
                uc.mem_write(bones + index * 172 + 0x6c, b'\x01\x00')
                native.write_words(uc, bones + index * 172 + 0x9c, 1800, 0x3aa3d70a, 0x3f400000)
            uc.mem_write(lookup, b'\xff' * 14)
            if key >= 0:
                uc.mem_write(lookup + key * 2, struct.pack('<H', mapped))
            native.write_words(uc, unit + 0xb84, key & 0xffffffff)
            native.write_words(uc, frame - 16, animation)
            for reg, value in [(UC_X86_REG_EBX, unit), (UC_X86_REG_EBP, frame), (UC_X86_REG_ESP, frame - 0x1000)]:
                uc.reg_write(reg, value)
            uc.emu_start(0x73917c, 0x739500, count=10000)
            assert uc.reg_read(UC_X86_REG_EIP) == 0x739500
            root = struct.unpack('<H', uc.mem_read(bones + 0x6c, 2))[0]
            spine = struct.unpack('<H', uc.mem_read(bones + 2 * 172 + 0x6c, 2))[0]
            rows.append(f'{animation} {key} {mapped} {root} {spine}')
    return rows


def capture_create_attack():
    """Execute 4D3890 and 98E560 with real readers and no hooks."""
    uc = native.emulator()
    record, packet, payload, target = [native.HEAP + i * 0x2000 for i in range(4)]
    rows = ['# createMovement retainedTarget consumed']
    for body in [b'\x00\x00', b'\x04\x00\x00', b'\x04\x00\x01\x07',
                 b'\x04\x00\x80\xf1', b'\x04\x00\xff\xbc\x9a\x78\x56\x34\x12\x30\xf1']:
        uc.mem_write(payload, body)
        native.write_words(uc, packet, 0, payload, 0, len(body), len(body), 0)
        native.write_words(uc, record + 0x2b8, 99, 88)
        uc.reg_write(UC_X86_REG_ECX, record)
        native.invoke(uc, 0x4d3890, [packet])
        consumed = native.read_words(uc, packet + 20, 1)[0]
        assert consumed == len(body)
        uc.reg_write(UC_X86_REG_ECX, target)
        native.invoke(uc, 0x98e560, [record])
        low, high = native.read_words(uc, target, 2)
        rows.append(f'{body.hex()} {low + (high << 32):016x} {consumed}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    routes, timers, bones, attacks, clears = capture_routes(), capture_timers(), capture_bone_clocks(), capture_attack_owner(), capture_death_clear()
    Path(args.output + '.routes.txt').write_text('\n'.join(routes) + '\n', encoding='utf-8')
    Path(args.output + '.timers.txt').write_text('\n'.join(timers) + '\n', encoding='utf-8')
    Path(args.output + '.bones.txt').write_text('\n'.join(bones) + '\n', encoding='utf-8')
    Path(args.output + '.attacks.txt').write_text('\n'.join(attacks) + '\n', encoding='utf-8')
    Path(args.output + '.clears.txt').write_text('\n'.join(clears) + '\n', encoding='utf-8')
    Path(args.output + '.creates.txt').write_text('\n'.join(capture_create_attack()) + '\n', encoding='utf-8')
    print(f'Captured {len(routes)-1} wound routes, {len(timers)-1} secondary timers, {len(bones)-1} bone clocks, {len(attacks)-1} attack packets and {len(clears)-1} death layer clears')
