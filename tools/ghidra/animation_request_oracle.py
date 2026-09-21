"""Execute stock sequence-prefetch and late-consumer dispatch boundaries.

The build-12340 instructions select variation requests, queue commands while the
model is unready, and dispatch retained consumers when a source completes. The saved word at waiter +0x10
is the timer offset (826B00 input), not the request wall-clock timestamp.
Hooks supply allocation, animation-ID normalization, source I/O admission and decoded
payload publication. Dispatch-only cases also hook final channel application;
channel cases execute original 826C40/826DD0/826B00 with a controlled CRT roll.
No operating-system or client entry point executes. This does not establish
sampling while pending, overlapping old blends, missing-companion cleanup or
payload retention policy.
"""
import argparse
import json
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as native


def returned(machine, value=0, arguments=0):
    """Return from one declared provider boundary using its native stack convention."""
    sp = machine.reg_read(UC_X86_REG_ESP)
    machine.reg_write(UC_X86_REG_EAX, value)
    machine.reg_write(UC_X86_REG_EIP, native.read_words(machine, sp, 1)[0])
    machine.reg_write(UC_X86_REG_ESP, sp + 4 + arguments)


def prefetch(ready, requested, entries):
    """Execute original linear sequence lookup and variation-chain admission."""
    uc = native.emulator()
    model, shared, scene, body, sequences, command = [native.HEAP + i * 0x1000 for i in range(6)]
    native.write_words(uc, model + 0x10, int(ready))
    native.write_words(uc, model + 0x28, scene, shared)
    native.write_words(uc, model + 0x38, model + 0x34)
    native.write_words(uc, scene + 0xc, 123456)
    native.write_words(uc, shared + 0x150, body)
    native.write_words(uc, body + 0x1c, len(entries), sequences, 0)
    for index, (animation, flags, following) in enumerate(entries):
        base = sequences + index * 64
        native.write_words(uc, base, animation, 1000, 0, flags)
        uc.mem_write(base + 0x3c, following.to_bytes(2, 'little'))
    requests = []
    allocations = []

    def providers(machine, address, _size, _context):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x76e540:
            size = native.read_words(machine, sp + 4, 1)[0]
            allocations.append(size)
            assert size == 0x50
            returned(machine, command, 16)
        elif address == 0x826350:
            # The fixture already supplies a normalized valid AnimationData ID.
            returned(machine, arguments=8)
        elif address == 0x83da10:
            assert machine.reg_read(UC_X86_REG_ECX) == shared
            requests.append(native.read_words(machine, sp + 4, 1)[0])
            returned(machine, arguments=4)

    uc.hook_add(UC_HOOK_CODE, providers)
    uc.reg_write(UC_X86_REG_ECX, model)
    native.invoke(uc, 0x827190, [requested])
    if ready:
        assert not allocations
        queued = None
    else:
        assert allocations == [0x50]
        assert not requests
        assert native.read_words(uc, model + 0x34, 2) == (command, command + 4)
        queued = list(native.read_words(uc, command, 4))
        assert queued == [14, 0, 123456, requested]
    return {'ready': ready, 'requested': requested, 'entries': entries,
            'source_requests': requests, 'queued_command': queued}


def joined(alias):
    """An existing pending consumer is updated in place, including an alias lookup."""
    uc = native.emulator()
    model, shared, body, sequences, request, waiter = [native.HEAP + i * 0x1000 for i in range(6)]
    native.write_words(uc, model + 0x2c, shared)
    native.write_words(uc, shared + 0x20, request)
    native.write_words(uc, body + 0x1c, 2, sequences)
    native.write_words(uc, sequences + 0xc, 0x10 | (0x40 if alias else 0))
    uc.mem_write(sequences + 0x3e, int(alias).to_bytes(2, 'little'))
    native.write_words(uc, sequences + 64 + 0xc, 0x10)
    uc.mem_write(sequences + 64 + 0x3e, (0).to_bytes(2, 'little'))
    native.write_words(uc, request + 0x10, int(alias))
    native.write_words(uc, request + 0x1c, waiter)
    native.write_words(uc, waiter + 8, model)
    uc.mem_write(waiter + 0xe, (8).to_bytes(2, 'little'))

    def providers(_machine, address, _size, _context):
        if address in (0x76e540, 0x83da10):
            raise AssertionError('joining an existing model consumer must not allocate or reread')

    uc.hook_add(UC_HOOK_CODE, providers)
    observed = []
    for stamp, flags in [(1000, 7), (2000, 0)]:
        uc.reg_write(UC_X86_REG_ECX, model)
        native.invoke(uc, 0x831c30, [0, body, 3, stamp, 0x3f400000, 2,
                                    flags & 1, flags & 2, flags & 4])
        assert native.read_words(uc, request + 0x1c, 1)[0] == waiter
        assert native.read_words(uc, waiter + 8, 1)[0] == model
        assert int.from_bytes(uc.mem_read(waiter + 0xc, 2), 'little') == 3
        assert int.from_bytes(uc.mem_read(waiter + 0xe, 2), 'little') == flags
        assert native.read_words(uc, waiter + 0x10, 3) == (stamp, 0x3f400000, 2)
        observed.append({'offset': stamp, 'flags': flags})
    return {'alias': alias, 'joined_updates': observed}


def completed(flags, admitted, original_offset, completion_time, apply_channels=False):
    """Execute dispatch from a completed sequence to one retained model consumer."""
    uc = native.emulator()
    shared, request, waiter, model, body, bone_defs, bone_state, scene, sequences = [
        native.HEAP + i * 0x1000 for i in range(9)]
    native.write_words(uc, shared + 0x150, body)
    native.write_words(uc, request + 0xc, shared, 1, 0, 0, waiter)
    native.write_words(uc, waiter + 4, 0, model)
    uc.mem_write(waiter + 0xc, (0).to_bytes(2, 'little') + flags.to_bytes(2, 'little'))
    native.write_words(uc, waiter + 0x10, original_offset, 0x3f400000, 2)
    native.write_words(uc, model + 0x28, scene, shared)
    native.write_words(uc, model + 0x94, bone_state)
    native.write_words(uc, scene + 0xc, completion_time)
    native.write_words(uc, body + 0x20, sequences)
    native.write_words(uc, body + 0x30, bone_defs)
    native.write_words(uc, bone_defs, 5)
    native.write_words(uc, sequences + 64, 17 | (3 << 16))
    if apply_channels:
        # A valid existing primary channel and no old blend. The incoming sequence
        # has distinct duration/blend bounds so both application paths are visible.
        native.write_words(uc, model + 0x10, 0x400001)
        uc.mem_write(model + 0x14, b'\xff\xff')
        native.write_words(uc, sequences + 64 + 4, 1000)
        native.write_words(uc, sequences + 64 + 0x14, 2, 7, 250)
        native.write_words(uc, bone_state + 0x48, 0, 100, 1100, 0x3f800000, 0x3f800000, 0, 1)
        uc.mem_write(bone_state + 0x6c, b'\xff\xff')
        native.write_words(uc, 0xd411c4, 0)
    calls = []

    def providers(machine, address, _size, _context):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x83ca90:
            calls.append(['publish', *native.read_words(machine, sp + 4, 2)])
            returned(machine, arguments=8)
        elif address == 0x8269c0:
            calls.append(['admit', *native.read_words(machine, sp + 4, 2)])
            returned(machine, int(admitted), 8)
        elif address == 0x826c40:
            calls.append(['primary', *native.read_words(machine, sp + 4, 6)])
            if not apply_channels:
                returned(machine, arguments=24)
        elif address == 0x826dd0:
            calls.append(['secondary', *native.read_words(machine, sp + 4, 5)])
            if not apply_channels:
                returned(machine, arguments=20)
        elif address == 0x88b867:
            # The timer's authored repeat range consumes the stock CRT source.
            returned(machine, 12345)
        elif address == 0x831bb0:
            calls.append(['withdraw', native.read_words(machine, sp + 4, 1)[0]])
            returned(machine, arguments=4)
        elif address == 0x83d370:
            calls.append(['detach', machine.reg_read(UC_X86_REG_ECX)])
            returned(machine)
        elif address == 0x76e5a0:
            calls.append(['free', native.read_words(machine, sp + 4, 1)[0]])
            returned(machine, arguments=16)

    uc.hook_add(UC_HOOK_CODE, providers)
    native.invoke(uc, 0x83d840, [request])
    assert calls[0][0] == 'publish'
    dispatched = [call for call in calls if call[0] in ('primary', 'secondary')]
    if flags & 8 or not admitted:
        assert not dispatched
        if flags & 8:
            assert any(call[0] == 'withdraw' for call in calls)
            assert not any(call[0] == 'admit' for call in calls)
    elif flags & 2:
        assert dispatched == [['primary', 1, 0, 2, original_offset, 0x3f400000, flags & 1]]
        assert native.read_words(uc, bone_state + 0x90, 1)[0] == 17
        assert int.from_bytes(uc.mem_read(bone_state + 0x94, 2), 'little') == 3
        assert bytes(uc.mem_read(bone_state + 0x4b, 1)) == bytes([flags & 4])
    else:
        assert dispatched == [['secondary', 1, 0, 2, original_offset, 0x3f400000]]
        assert bytes(uc.mem_read(bone_state + 0x6f, 1)) == bytes([flags & 4])
    if admitted and not flags & 8:
        assert native.read_words(uc, waiter + 8, 1)[0] == 0
    assert native.read_words(uc, shared + 8, 1)[0] == 0
    assert calls[-2:] == [['detach', request], ['free', request]]
    result = {'flags': flags, 'admitted': admitted, 'original_offset': original_offset,
              'completion_time': completion_time, 'calls': calls}
    if apply_channels:
        result['channels'] = {
            'primary': list(native.read_words(uc, bone_state + 0x48, 7)),
            'secondary': list(native.read_words(uc, bone_state + 0x6c, 7)),
            'blend': list(native.read_words(uc, bone_state + 0x9c, 3)),
        }
    return result


def capture(executable, output):
    """Write exact provider-boundary observations only after all assertions pass."""
    native.initialize(executable)
    entries = [(17, 0, 1), (17, 0x10, 2), (17, 0x20, 3), (17, 0x40, 0xffff), (42, 0, 0xffff)]
    rows = [prefetch(True, 17, entries), prefetch(True, 99, entries), prefetch(False, 17, entries)]
    assert rows[0]['source_requests'] == [0, 3]
    assert rows[1]['source_requests'] == []
    rows.extend([joined(False), joined(True)])
    early = completed(3, True, 123, 1000)
    late = completed(3, True, 123, 9000)
    assert early['calls'] == late['calls']
    rows.extend([early, late])
    for flags in range(16):
        for admitted in (False, True):
            rows.append(completed(flags, admitted, 1000, 9000))
    for flags in range(16):
        for admitted in (False, True):
            reference = None
            for tick in (1000, 9000, 0xffffff00):
                row = completed(flags, admitted, 123, tick, True)
                channels = row['channels']
                if not admitted or flags & 8:
                    assert channels == {
                        'primary': [0, 100, 1100, 0x3f800000, 0x3f800000, 0, 1],
                        'secondary': [0xffff, 0, 0, 0, 0, 0, 0],
                        'blend': [0, 0, 0],
                    }
                else:
                    selected = 'primary' if flags & 2 else 'secondary'
                    if reference is not None:
                        # Delaying completion translates both timer bounds by the
                        # same wrapping scene delta; speed/offset/repeat state stays.
                        expected = reference['channels'][selected].copy()
                        delta = tick - reference['completion_time']
                        expected[1] = (expected[1] + delta) & 0xffffffff
                        expected[2] = (expected[2] + delta) & 0xffffffff
                        assert channels[selected] == expected
                    if flags & 2:
                        if flags & 1:
                            assert channels['secondary'] == [
                                0, 100, 1100, 0x3f800000, 0x3f800000, 0, 1]
                            assert channels['blend'][0] == (tick + 250) & 0xffffffff
                        else:
                            assert channels['secondary'][0] == 0xffff
                    else:
                        assert channels['primary'] == [
                            0, 100, 1100, 0x3f800000, 0x3f800000, 0, 1]
                        assert channels['blend'][0] == (tick + 1000) & 0xffffffff
                reference = row
                rows.append(row)
    output.write_text(json.dumps({'image_sha256': 'aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
                                  'cases': rows}, indent=2) + '\n', encoding='utf-8')
    print(f'passed {len(rows)} native request/consumer cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
