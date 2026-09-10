"""Capture stock local WMO frusta and ordered resident MOBA selection.

The fingerprinted image executes 790E20, 78FB00/983F40 and 7A7630 unhooked.
The four renderer callbacks execute their original selection loops. The sole
hook skips texture/shader/GPU submission after the native accepted flag store.
This bounds the evidence to resident drawable batch selection, not shader
setup, texture residency, or callback dispatch.
"""
import argparse
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EDI,
    UC_X86_REG_EIP, UC_X86_REG_ESI, UC_X86_REG_ESP,
)

import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from world_model_portal_projection_oracle import words


LOOPS = {
    'normal': (0x7ac730, 0x7ac770, 0x7ac9c3, 0x7ac9db, UC_X86_REG_EDI),
    'colored': (0x7aca99, 0x7acaf0, 0x7acfe3, 0x7ad001, UC_X86_REG_EDI),
    'unified': (0x7a9432, 0x7a9474, 0x7a9bb3, 0x7a9bce, UC_X86_REG_ESI),
    'untextured': (0x7a9c9c, 0x7a9ccc, 0x7a9cff, 0x7a9d0d, UC_X86_REG_ESI),
}


def select(u, group, batches, ordinal, variant='normal'):
    """Run original bounds, deduplication, native count and loop order."""
    selected = []
    start, accepted, next_batch, end, batch_register = LOOPS[variant]

    def submission(uc, address, size, user):
        if address == accepted:
            selected.append((uc.reg_read(batch_register) - batches) // 24)
            uc.reg_write(UC_X86_REG_EIP, next_batch)
        elif address == end:
            uc.emu_stop()

    bp = n.STACK + 0x10000
    n.write_words(u, bp + 8, group, ordinal)
    u.reg_write(UC_X86_REG_EBP, bp)
    u.reg_write(UC_X86_REG_ESP, bp - 0x100)
    u.reg_write(UC_X86_REG_EBX, group)
    u.reg_write(UC_X86_REG_ESI, 0)
    if variant == 'colored':
        u.reg_write(UC_X86_REG_ESI, group)
        n.write_words(u, bp - 0x20, 0)
    elif variant in ('unified', 'untextured'):
        u.reg_write(UC_X86_REG_EDI, group)
        u.reg_write(UC_X86_REG_EBX, 0)
        u.reg_write(UC_X86_REG_ESI, batches)
    hook = u.hook_add(UC_HOOK_CODE, submission)
    try:
        u.emu_start(start, n.STOP, timeout=1_000_000, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == end
    finally:
        u.hook_del(hook)
    return selected


def main():
    """Generate float-store fixtures and repeated-window native draw sequences."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('frusta', type=Path)
    parser.add_argument('batches', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    window_ptr, matrix_ptr, group, batches = [n.HEAP + x for x in (0, 0x100, 0x1000, 0x2000)]
    n.write_words(u, 0xcd8798, 0)
    frames = [floats(line) for line in args.frames.read_text().splitlines() if not line.startswith('#')]
    matrices = [
        [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
        [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -103.125, 21.75, -.125, 1],
        [.6, .8, 0, 0, -.8, .6, 0, 0, 0, 0, 1, 0, 1100.125, -4500.25, 150, 1],
        [.3125, .75, 0, 0, -.75, .3125, 0, 0, 0, 0, .8125, 0, -.125, .25, -123.75, 1],
        [1, .125, -.25, 0, .25, 1, .125, 0, 0, .25, 1, 0, 16000.125, -17000.25, 123.75, 1],
        [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, .125, -.25, .375, 1],
    ]
    windows = [[0, 0, 1, .5], [.5, .5, 1, 1], [0, .5, .5, 1],
               [.2, .3, .7, .8], [0, 0, 1, 1], [0, 0, 1, .5]]
    frusta = ['# frame index; matrix16 window4 local corners24 planes24 (hex f32); originals 790E20, 78FB00, 983F40']
    sequences = ['# frame index; matrix16 (hex f32); batch count; bounds6 initial flag (decimal) per batch; window count; window4 (hex f32), selected count, selected ids (decimal) per window; final flags (decimal)']
    rng = random.Random(0x7ac6a0)
    for index in range(0, len(frames), 54):
        for matrix in matrices:
            n.write_floats(u, matrix_ptr, matrix)
            clips = []
            for window in windows:
                n.write_floats(u, 0xcdb108, frames[index][64:88])
                n.write_floats(u, window_ptr, window)
                n.invoke(u, 0x790e20, [0xcdb108, window_ptr])
                n.invoke(u, 0x78fb00, [matrix_ptr])
                corners = n.read_floats(u, 0xcdb168 + 0x60, 24)
                planes = n.read_floats(u, 0xcdb168, 24)
                frusta.append(f'{index} ' + words(matrix + window + corners + planes))
                clips.append(bytes(u.mem_read(0xcdb168, 0xfc)))
            # Spread authored integral bounds over the full local frustum, with
            # tiny boxes and enclosing boxes to exercise independent windows.
            full = struct.unpack_from('<24f', clips[4], 0x60)
            low = [min(full[a::3]) for a in range(3)]
            high = [max(full[a::3]) for a in range(3)]
            boxes = [[-32768] * 3 + [32767] * 3, [0] * 6]
            for _ in range(46):
                center = [rng.uniform(low[a], high[a]) for a in range(3)]
                extent = [rng.choice([0, 1, 8, 128]) for _ in range(3)]
                boxes.append([max(-32768, min(32767, round(center[a] + sign * extent[a])))
                              for sign in (-1, 1) for a in range(3)])
            flags = [rng.randrange(256) for _ in boxes]
            u.mem_write(group + 0x60, struct.pack('<H', len(boxes)))
            n.write_words(u, group + 0xf8, batches)
            initial = b''.join(struct.pack('<6h10xBB', *box, flag, 0)
                               for box, flag in zip(boxes, flags))
            u.mem_write(batches, initial)
            row = [str(index), words(matrix), str(len(boxes))]
            row.extend(' '.join(map(str, box + [flag])) for box, flag in zip(boxes, flags))
            row.append(str(len(windows)))
            all_selected = []
            baseline = []
            for ordinal, (window, clip) in enumerate(zip(windows, clips)):
                u.mem_write(0xcdb168, clip)
                selected = select(u, group, batches, ordinal)
                baseline.append(selected)
                assert not set(selected).intersection(all_selected)
                all_selected.extend(selected)
                row.extend([words(window), str(len(selected)), ' '.join(map(str, selected))])
            final = [u.mem_read(batches + 24 * i + 22, 1)[0] for i in range(len(boxes))]
            assert all((a & 15) == (b & 15) for a, b in zip(flags, final))
            for variant in LOOPS:
                for count in (0, 1, 17, len(boxes)):
                    u.mem_write(batches, initial)
                    # Deliberately disagree: each original must read its own
                    # count field, rather than accidentally sharing a bound.
                    u.mem_write(group + 0x60, struct.pack('<H', count if variant == 'normal' else 3))
                    n.write_words(u, group + 0x16c, count if variant != 'normal' else 3)
                    for ordinal, clip in enumerate(clips):
                        u.mem_write(0xcdb168, clip)
                        actual = select(u, group, batches, ordinal, variant)
                        assert actual == [i for i in baseline[ordinal] if i < count], (variant, count, ordinal)
                    actual_flags = [u.mem_read(batches + 24 * i + 22, 1)[0] for i in range(len(boxes))]
                    assert actual_flags == final[:count] + flags[count:], (variant, count)
            row.extend(map(str, final))
            sequences.append(' '.join(row))
    args.frusta.write_text('\n'.join(frusta) + '\n', encoding='utf-8')
    args.batches.write_text('\n'.join(sequences) + '\n', encoding='utf-8')
    print(f'Captured {len(frusta)-1} local frusta and {len(sequences)-1} native batch sequences; '
          'all four renderer loops agree at four independent count limits')


if __name__ == '__main__':
    main()
