"""Capture the original 76F0D0 decimal parser used by CVar's cached integer."""
import argparse
import hashlib
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EAX
import wmo_registration_oracle as n


def capture(executable, output):
    n.initialize(executable)
    machine = n.emulator()
    rows = [f'# Wow.exe sha256 {hashlib.sha256(n.data).hexdigest()}',
            '# UTF-8 hex (- means empty), native integer as u32 hex']
    for value in ['', '0', '1', '-1', '-', '+1', ' 1', '\t1', '\n1', '1 ',
                  '0.5', '-0.9', '1.25', '-1.25', '.5', '1e2', '1foo',
                  '0x10', 'NaN', 'inf', 'true', 'false', '0000123', '--1',
                  '2147483647', '2147483648', '-2147483648', '-2147483649',
                  '4294967295', '4294967296', '4294967297', '-4294967297',
                  '99999999999999999999999999999', '١', '１', '1\x002']:
        encoded = value.encode('utf-8')
        machine.mem_write(n.HEAP, encoded + b'\0')
        n.invoke(machine, 0x76f0d0, [n.HEAP])
        rows.append(f'{encoded.hex() or "-"} {machine.reg_read(UC_X86_REG_EAX):08x}')
    output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} native CVar integer cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
