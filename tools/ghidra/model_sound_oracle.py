"""Capture model callback options by executing unmodified build-12340 code.

Only downstream playback, live-handle/nearby tests, and the global mode query
are intercepted. Callback branching and 4C5990 option initialization execute
from the fingerprinted executable; the harness never constructs their options.
"""
import argparse
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    observed = []
    live, nearby = 0, 0

    def dependency(u, address, _size, _data):
        if address not in (0x4c5be0, 0x4cfe00, 0x422130, 0x4c6a40, 0x4c6390):
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        value = 0
        if address == 0x4c5be0:
            value = live
        elif address == 0x4cfe00:
            value = nearby
        elif address == 0x4c6a40:
            entry, position, handle, options = native.read_words(u, sp + 4, 4)
            # Null options ask the downstream wrapper to initialize defaults.
            values = native.read_words(u, options, 19) if options else []
            observed.append('play ' + ' '.join(f'{word:x}' for word in (entry, bool(handle), *values)))
        elif address == 0x4c6390:
            observed.append('stop ' + ' '.join(f'{word:x}' for word in native.read_words(u, sp + 8, 3)))
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, dependency)
    rows = ['# native 70C050 / 7BD5A0: kind identifier live nearby -> action entry retained options[0..18]']
    for kind, address in [('gameobject', 0x70c050), ('doodad', 0x7bd5a0)]:
        for identifier in ('$DSL', '$DSO', '$SND', '$DSE'):
            for live, nearby in [(0, 0), (1, 0), (0, 1)]:
                observed.clear()
                event = int.from_bytes(identifier.encode('ascii'), 'little')
                if kind == 'gameobject':
                    uc.reg_write(UC_X86_REG_ECX, event)
                    invoke(uc, address, [77, native.HEAP + 0x1000, native.HEAP])
                else:
                    invoke(uc, address, [0, 0, event, 77, native.HEAP + 0x1000, 0, native.HEAP])
                rows.append(f'{kind} {identifier} {live} {nearby} ' + (';'.join(observed) or 'none'))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original model callbacks')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
