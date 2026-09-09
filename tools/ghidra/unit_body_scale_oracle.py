"""Unmodified 722AE0/71C110/71C050 scale probe with controlled DBC providers."""
import argparse
import random
import struct
from pathlib import Path
import wmo_registration_oracle as n
from unicorn.x86_const import UC_X86_REG_ECX

def capture(display_scale, model_scale, extra_scale, family, level, pet, gender=0, absent=0):
    """Execute all scale providers unchanged; only their DBC rows are controlled."""
    unit, fields, template, display, model, family_row, extra, race, male, female, tables, output = [
        n.HEAP + offset for offset in (
            0, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000,
            0x7000, 0x8000, 0x9000, 0x9100, 0x10000, 0x11000,
        )
    ]
    uc = n.emulator()

    def table(minimum, maximum, pointer, start, rows, bank):
        """Publish a native DBC row-pointer bank and its inclusive key range."""
        n.write_words(uc, minimum, start)
        n.write_words(uc, maximum, start + len(rows) - 1)
        n.write_words(uc, pointer, bank)
        n.write_words(uc, bank, *rows)

    n.write_words(uc, unit + 0xD0, fields)
    n.write_words(uc, unit + 0x964, template if family else 0)
    n.write_words(uc, template + 0x14, 400)
    n.write_words(uc, fields + 0xC0, level)
    n.write_words(uc, fields + 0x114, pet)
    table(0xAD34C8, 0xAD34C4, 0xAD34D8, 100, [
        display if absent != 1 else 0,
        male if absent != 5 else 0,
        female if absent != 5 else 0,
    ], tables)
    table(0xAD3510, 0xAD350C, 0xAD3520, 200, [model if absent != 2 else 0], tables + 0x100)
    table(0xAD34A4, 0xAD34A0, 0xAD34B4, 300, [extra if absent != 3 else 0], tables + 0x200)
    table(0xAD3438, 0xAD3434, 0xAD3448, 1, [race if absent != 4 else 0], tables + 0x300)
    table(0xAD34EC, 0xAD34E8, 0xAD34FC, 400, [family_row if absent != 6 else 0], tables + 0x400)
    n.write_words(uc, display + 4, 200)
    n.write_words(uc, display + 0xC, 300 if extra_scale is not None else 0)
    n.write_floats(uc, display + 0x10, [display_scale])
    n.write_floats(uc, model + 0x10, [model_scale])
    n.write_words(uc, extra + 4, 1, gender)
    n.write_words(uc, race + 0x10, 101, 102)
    n.write_floats(uc, male + 0x10, [extra_scale or 0])
    n.write_floats(uc, female + 0x10, [extra_scale or 0])
    if family:
        minimum_scale, minimum_level, maximum_scale, maximum_level = family
        n.write_floats(uc, family_row + 4, [minimum_scale])
        n.write_words(uc, family_row + 8, minimum_level)
        n.write_floats(uc, family_row + 12, [maximum_scale])
        n.write_words(uc, family_row + 16, maximum_level)
    uc.reg_write(UC_X86_REG_ECX, unit)
    n.invoke(uc, 0x722AE0, [100])
    # Publish ST(0) exactly as 73FCC0's final FSTP dword body-scale store.
    uc.mem_write(n.STOP, b'\xd9\x1d' + struct.pack('<I', output))
    uc.emu_start(n.STOP, n.STOP + 6)
    return n.read_floats(uc, output, 1)[0]


def cases():
    """Exercise ordinary/pet branches, signed endpoints, missing providers and rounding."""
    for family in [None, (.2,1,1.5,60), (.2,10,1.5,10), (.2,60,1.5,1), (0.,1,0.,80)]:
        for pet in [0, 77]:
            for level in [0,1,10,30,60,80,0xffffffff]:
                yield (.4,1.3,1.12,family,level,pet,0,0)
    for extra in [None, 0., .8, 1.12]:
        for gender in [0,1,2]:
            for absent in range(7):
                yield (.4,1.3,extra,(.2,1,1.5,60),20,0,gender,absent)
    for display in [-1.,0.,.1,1.,2.]:
        for model in [-1.,0.,.7,1.,2.]:
            yield (display,model,None,None,1,0,0,0)
    rng = random.Random(12340)
    for _ in range(150):
        family = (rng.uniform(.05,2),1,rng.uniform(.05,3),80)
        yield (rng.uniform(.1,2),rng.uniform(.1,2),rng.uniform(.1,2),family,rng.randrange(100),rng.choice([0,9]),rng.randrange(2),0)


def main():
    """Write portable input words and the original client's final float stores."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    n.initialize(args.exe)
    records = []
    for display,model,extra,family,level,pet,gender,absent in cases():
        result = capture(display,model,extra,family,level,pet,gender,absent)
        lo,ll,hi,hl = family or (0.,0,0.,0)
        records.append(struct.pack('<fffIIfIfIIIIIf',display,model,extra or 0.,int(extra is not None),int(family is not None),lo,ll,hi,hl,level,pet,gender,absent,result))
    Path(args.output).write_bytes(b'UBS12340'+struct.pack('<I',len(records))+b''.join(records))
    print(f'captured {len(records)} unhooked native unit body scales')


if __name__ == '__main__':
    main()
