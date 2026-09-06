"""Capture native M2 timer construction, speed updates, poses and boundaries.

Runs original 826B00/827000/832260. A supplied CRT roll is the only timer
construction hook. Pose sampling runs the complete primary arithmetic block
82F476..82F506 with its sequence, bone timer and scene tick inputs; no model
matrix or track sampling is involved. Callback scanning intercepts event
enumeration (no authored event keys) and queue insertion, retaining the native
deadline arithmetic. Event mapping runs the arithmetic block 8310AC..8310DC.
Variation callbacks execute 831FC0 with supplied descriptor/sequence lookup,
then execute original 826B00 using the callback's offset and retained speed.
No external animation loading, matrix composition or bone layers are tested.
Requires the fingerprinted, locally owned build-12340 PE.
"""
import argparse
from pathlib import Path
import struct
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EDI, UC_X86_REG_EBP, UC_X86_REG_EIP, UC_X86_REG_ESP


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    model, resource, data, scene, sequence, bone = [native.HEAP + i * 0x1000 for i in range(6)]
    native.write_words(uc, model + 0x10, 0x400001, 0)
    native.write_words(uc, model + 0x28, scene, resource)
    native.write_words(uc, model + 0x94, bone)
    native.write_words(uc, resource + 0x150, data)
    native.write_words(uc, data + 0x20, sequence)
    native.write_words(uc, data + 0x2c, 1)
    uc.mem_write(bone + 0x96, b'\xff\xff')
    native.write_words(uc, 0xd411c4, 0)
    boundary, variation, selected_mode = None, None, 0
    callback, timestamps = native.HEAP + 0x6000, native.HEAP + 0x7000
    def dependencies(u, address, _size, _data):
        nonlocal boundary, variation
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x88b867:
            value, pop = 12345, 0
        elif address == 0x830fb0:
            value, pop = native.read_words(u, sp + 24, 1)[0], 28
        elif address == 0x82e790:
            _, now, overdue, _ = native.read_words(u, sp + 4, 4)
            boundary = (now - overdue) & 0xffffffff
            value, pop = boundary, 16
        elif address == 0x826350:
            native.write_words(u, native.read_words(u, sp + 4, 1)[0], selected_mode << 16)
            value, pop = 0, 8
        elif address == 0x8260c0:
            value, pop = 0, 12
        elif address == 0x826e60:
            native.write_words(u, native.read_words(u, sp + 4, 1)[0], 0)
            value, pop = 0, 8
        elif address == 0x826c40:
            variation = native.read_words(u, sp + 4, 6)
            value, pop = 0, 24
        else:
            return
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    uc.hook_add(UC_HOOK_CODE, dependencies)

    def sample(time):
        uc.reg_write(UC_X86_REG_EAX, time)
        uc.reg_write(UC_X86_REG_ECX, sequence)
        uc.reg_write(UC_X86_REG_EDI, bone)
        uc.reg_write(UC_X86_REG_EBP, native.STACK + 0x10000)
        uc.emu_start(0x82f476, 0x82f506, timeout=1000000, count=1000)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x82f506
        return uc.reg_read(UC_X86_REG_EAX)

    def timer_words():
        return ' '.join(str(value) for value in native.read_words(uc, bone + 0x4c, 6))

    def event_tick(timestamp, cycle_offset):
        native.write_words(uc, timestamps, timestamp)
        native.write_words(uc, native.STACK + 0x10000 - 8, cycle_offset)
        uc.reg_write(UC_X86_REG_EAX, 0)
        uc.reg_write(UC_X86_REG_ECX, bone)
        uc.reg_write(UC_X86_REG_EDX, timestamps)
        uc.reg_write(UC_X86_REG_EBP, native.STACK + 0x10000)
        uc.emu_start(0x8310ac, 0x8310dc, timeout=1000000, count=1000)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x8310dc
        return uc.reg_read(UC_X86_REG_EDI)

    rows = ['# setup duration mode speedBits offset scene phase -> start end speedBits inverseBits initial cycles',
            '# update duration mode speedBits offset scene phase newScene newSpeedBits -> same timer words',
            '# pose duration mode speedBits offset scene phase time -> primary time',
            '# boundary duration mode speedBits offset scene phase previous current -> deadline (-1 none)',
            '# event duration mode speedBits offset scene phase key cycleOffset -> scene tick',
            '# variation duration mode speedBits offset scene phase newScene overdue -> timer words']
    for duration in [0, 997, 1000, 16777217]:
        native.write_words(uc, sequence + 4, duration)
        native.write_words(uc, sequence + 0x14, 2, 7)
        for mode in range(4):
            for speed in [0., 0.000001, 0.25, 0.7, 1., 1.25, 2., 3., -0.5, -2.]:
                for offset, now, phase in [(0, 1000, 0), (123, 1000, 1), (-234, 0xfffffff0, 0)]:
                    native.write_words(uc, scene + 12, now)
                    native.write_words(uc, scene + 28, phase * 4)
                    uc.reg_write(UC_X86_REG_ECX, model)
                    invoke(uc, 0x826b00, [0, mode << 16, offset & 0xffffffff, bits(speed), bone + 0x40])
                    key = f'{duration} {mode} {bits(speed)} {offset} {now} {phase}'
                    rows.append(f'setup {key} {timer_words()}')
                    original = bytes(uc.mem_read(bone + 0x48, 28))
                    if duration < 2000:
                        for flags in [0x20, 0x21]:
                            native.write_words(uc, sequence + 12, flags)
                            for delta in [-1, 0, 1, 123, 1000, 4001]:
                                time = (now + delta) & 0xffffffff
                                rows.append(f'pose {key} {flags} {time} {sample(time)}')
                            for first, last in [(0, 1), (1, 500), (500, 1000), (1000, 4001)]:
                                previous, current = (now + first) & 0xffffffff, (now + last) & 0xffffffff
                                native.write_words(uc, scene + 12, current, (current - previous) & 0xffffffff)
                                uc.reg_write(UC_X86_REG_ECX, model)
                                boundary = None
                                invoke(uc, 0x832260, [])
                                rows.append(f'boundary {key} {flags} {previous} {current} {boundary if boundary is not None else -1}')
                        for timestamp in [0, 125, 997]:
                            for cycle_offset in [0, 1234]:
                                rows.append(f'event {key} {timestamp} {cycle_offset} {event_tick(timestamp, cycle_offset)}')
                    for new_speed in [0., 0.7, 2.]:
                        uc.mem_write(bone + 0x48, original)
                        later = (now + 333) & 0xffffffff
                        native.write_words(uc, scene + 12, later)
                        uc.reg_write(UC_X86_REG_ECX, model)
                        invoke(uc, 0x827000, [0xffffffff, bits(new_speed)])
                        rows.append(f'update {key} {later} {bits(new_speed)} {timer_words()}')
                    uc.mem_write(bone + 0x48, original)
                    uc.mem_write(bone + 0x4b, b'\x01')
                    uc.mem_write(sequence + 2, b'\x01\x00')
                    native.write_words(uc, sequence + 12, 0x20)
                    native.write_words(uc, callback + 8, 0xffffffff, 0, 0, 0, 0, native.read_words(uc, bone + 0x4c, 1)[0])
                    native.write_words(uc, callback + 0x24, 333)
                    selected_mode, variation = mode, None
                    uc.reg_write(UC_X86_REG_ECX, model)
                    invoke(uc, 0x831fc0, [callback])
                    assert variation is not None, key
                    new_scene = (now + 333) & 0xffffffff
                    native.write_words(uc, scene + 12, new_scene)
                    native.write_words(uc, scene + 28, 4)
                    uc.reg_write(UC_X86_REG_ECX, model)
                    invoke(uc, 0x826b00, [0, variation[2], variation[3], variation[4], bone + 0x40])
                    rows.append(f'variation {key} {new_scene} 333 {timer_words()}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {sum(not row.startswith("#") for row in rows)} original M2 speed cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
