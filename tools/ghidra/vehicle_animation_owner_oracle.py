"""Pinned Vehicle_C registration/completion with the original seated consumer.

Executes 756D10, 756CD0, 757280 and 747980. GUID lookup supplies resident
passengers; model replay, key release and Unit_C resume are captured boundaries.
This does not execute entry/exit spell consumers or the CM2Model setter.
"""
import sys
from pathlib import Path
import wmo_registration_oracle as n
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

n.initialize(sys.argv[1])
u = n.emulator()
vehicle, unit, model, actor = [n.HEAP + 0x4000 * i for i in range(4)]
residents = set()
calls = []


def dependencies(u, address, size, data):
    sp = u.reg_read(UC_X86_REG_ESP)
    if address == 0x4d4db0:
        guid, high = n.read_words(u, sp + 4, 2)
        assert high == 0
        result, pop = (actor if guid in residents else 0), 0
    elif address == 0x735820:
        args = n.read_words(u, sp + 4, 9)
        assert args == (model, replay_key, 115, 0xffffffff, 49, 0x3f800000, 1, 1, 0), args
        assert n.read_words(u, 0xca1608, 1)[0] == 1
        calls.append(1)
        result, pop = 0, 36
    elif address == 0x757420:
        calls.append(2)
        result, pop = 0, 8
    elif address == 0x73ac30:
        calls.append(3)
        result, pop = 0, 8
    else:
        return
    u.reg_write(UC_X86_REG_EAX, result)
    u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
    u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)


u.hook_add(UC_HOOK_CODE, dependencies)
rows = [
    '# Original 756D10/756CD0/757280/747980; GUID residency supplied; terminal model/Unit calls captured.',
    '# key count missing reason controls last_registration owned_low owned_high action owner_guids[16]',
]
for key in [-1, 4, 26, 34, 35]:
    normalized = 26 if key < 0 or key > 34 else key
    replay_key = 0xffffffff if normalized == 26 else normalized
    for count in [0, 1, 2, 16, 17]:
        for missing in [0, 1, 2]:
            for reason in [0, 1]:
                residents.clear()
                residents.update(guid for guid in range(1, count + 1)
                                 if missing == 0 or (missing == 1 and guid != 1))
                u.mem_write(vehicle, bytes(0x400))
                n.write_words(u, vehicle + 4, unit)
                n.write_words(u, vehicle + 0x58, 0xffffffff, 0xffffffff)
                n.write_words(u, 0xca1608, 0)
                registered = 0
                for guid in range(1, count + 1):
                    u.reg_write(UC_X86_REG_ECX, vehicle)
                    invoke(u, 0x756d10, [guid, 0, key & 0xffffffff, 0x747980])
                    registered = u.reg_read(UC_X86_REG_EAX)
                u.reg_write(UC_X86_REG_ECX, vehicle)
                invoke(u, 0x756cd0, [key & 0xffffffff])
                controls = int(u.reg_read(UC_X86_REG_EAX) != 0)
                calls.clear()
                u.reg_write(UC_X86_REG_ECX, vehicle)
                invoke(u, 0x757280, [model, key & 0xffffffff, 115, reason, 49])
                bits = n.read_words(u, vehicle + 0x58, 2)
                owners = [n.read_words(u, vehicle + 0x60 + i * 16, 1)[0] for i in range(16)]
                rows.append(' '.join(map(str, [key, count, missing, reason, controls, registered,
                                              *bits, calls[0] if calls else 0, *owners])))
Path(sys.argv[2]).write_text('\n'.join(rows) + '\n')
print('Captured', len(rows) - 2, 'original seated-owner registration/completion cases')
