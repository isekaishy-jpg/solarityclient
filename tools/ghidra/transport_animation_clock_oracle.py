"""Capture build-12340 type-11 clock creation, callbacks, and endpoint sampling.

Original 7105D0, 710640, 710780, 710820, 7106D0 and 70DA40 execute in the
fingerprinted PE. Only the wall-clock and unrelated geometry providers are
hooked. No client entry point, operating system, or model code executes.
"""
import argparse
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as native


def capture(period, split, initial, raw, progress, actions):
    uc = native.emulator()
    owner, go, fields, positions, output = [native.HEAP + x for x in (0, 0x1000, 0x2000, 0x3000, 0x4000)]
    native.write_words(uc, owner + 4, go)
    native.write_words(uc, go + 0xd0, fields)
    native.write_words(uc, fields + 0x20, progress << 16)
    native.write_words(uc, fields + 0x28, split, initial & 255)
    native.write_words(uc, owner + 0x38, initial & 0xffffffff)
    native.write_words(uc, owner + 0x40, positions if period else 0, 1 if period else 0)
    native.write_words(uc, positions + 8, period or 0)
    current_time = raw
    sampled_phase = None

    def intercept(u, address, size, data):
        nonlocal sampled_phase
        if address not in (0x86ae20, 0x70daa0, 0x70dc10):
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        values = native.read_words(u, sp, 3)
        if address == 0x86ae20:
            result, pop = current_time, 0
        else:
            result, pop = values[1], 8
            if address == 0x70daa0:
                sampled_phase = values[2]
                native.write_floats(u, result, [0., 0., 0.])
            else:
                native.write_floats(u, result, [0., 0., 0., 1.])
        u.reg_write(UC_X86_REG_EAX, result)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
        u.reg_write(UC_X86_REG_EIP, values[0])

    uc.hook_add(UC_HOOK_CODE, intercept)

    def call(address, *args):
        uc.reg_write(UC_X86_REG_ECX, owner)
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, *args)
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.emu_start(address, native.STOP, count=100_000)
        if uc.reg_read(UC_X86_REG_EIP) != native.STOP:
            raise RuntimeError(f'bounded clock probe did not return from {address:x}')
        return uc.reg_read(UC_X86_REG_EAX)

    state = initial
    call(0x7105d0, raw)
    observations = []
    for kind, current_time, value in [('sample', raw, None)] + actions:
        sampled_phase = None
        old = state
        if kind == 'state':
            state = value
            native.write_words(uc, fields + 0x2c, state & 255)
            call(0x710820, old & 0xffffffff, state & 0xffffffff)
        elif kind == 'split':
            split = value
            native.write_words(uc, fields + 0x28, split)
        elif kind == 'sample':
            call(0x7106d0, current_time, output, output + 16)
        cached = native.read_words(uc, owner + 0x38, 1)[0]
        observations.append(dict(kind=kind, raw=current_time, value=value,
                                 sample=sampled_phase,
                                 phase=call(0x710640, state & 0xffffffff, current_time),
                                 passenger=call(0x710640, cached, current_time)))
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    cases = []
    for period in (None, 1000, 10001, 0xffffffff):
        for split in (0, 400, 900):
            for state in (0, 1, -1):
                for progress in (0, 1, 32768, 65534, 65535):
                    raw = 0xfffffff0
                    actions = []
                    for offset in (0, 1, 379, 380, 399, 400, 949, 950, 999, 1000, 1100, 2400):
                        time = (raw + offset) & 0xffffffff
                        actions.append(('sample', time, None))
                        if offset in (379, 380, 949, 999, 1100):
                            actions.append(('state', time, (0 if state else 1) if offset != 1100 else state))
                    inputs = dict(period=period, split=split, state=state, raw=raw, progress=progress)
                    cases.append(dict(**inputs, observations=capture(period, split, state, raw, progress, actions)))
    # Each callback starts fresh around the exact 95% branch; sample afterwards
    # must cover both reversal endpoints and the opposite interval's duration.
    for state in (0, 1, -1):
        for elapsed in (0, 1, 2, 3, 379, 380, 381, 569, 570, 571, 600, 900):
            actions = [('state', 10000 + elapsed, 1 if state == 0 else 0)]
            actions += [('sample', 10000 + elapsed + n, None) for n in (0, 1, 2, 100, 399, 400, 601, 1000)]
            cases.append(dict(period=1000, split=400, state=state, raw=10000, progress=0,
                              observations=capture(1000, 400, state, 10000, 0, actions)))
    lines = ['# native 7105D0/710640/710780/710820/7106D0/70DA40; pinned Wow.exe']
    for case in cases:
        lines.append('case ' + ' '.join(str(case[k] or 0) for k in ('period', 'split', 'state', 'raw', 'progress')))
        for item in case['observations']:
            lines.append(' '.join(str(item[k]) if item[k] is not None else '-' for k in
                                  ('kind', 'raw', 'value', 'sample', 'phase', 'passenger')))
    Path(args.output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'captured {len(cases)} native clock traces')


if __name__ == '__main__':
    main()
