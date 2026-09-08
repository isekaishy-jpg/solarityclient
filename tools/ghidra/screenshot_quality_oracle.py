"""Capture build-12340 screenshot quality conversion from unchanged 4A84A0."""
import argparse
from pathlib import Path
import wmo_registration_oracle as n

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    from unicorn.x86_const import UC_X86_REG_EAX
    rows = ['# hex CVar string -> native JPEG quality (76F0D0 then 4A84A0)']
    values = list(map(str, [-2147483648, -100, -1, *range(13), 100, 2147483647]))
    values += ['3.5', '7suffix', ' 8', '+8', '-2.5', 'invalid', '4294967297', '-4294967297']
    for value in values:
        u.mem_write(n.HEAP, value.encode() + b'\0')
        n.invoke(u, 0x76f0d0, [n.HEAP])
        n.invoke(u, 0x4a84a0, [u.reg_read(UC_X86_REG_EAX)])
        rows.append(f'{value.encode().hex()} {n.read_words(u, 0xac1b98, 1)[0]}')
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native screenshot qualities')
