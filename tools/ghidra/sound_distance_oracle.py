"""Execute build 12340's custom FMOD rolloff callback without replaced callees."""

import argparse
from pathlib import Path
import struct

import wmo_registration_oracle as native


def capture(executable, output):
    """Store original x87 return values as the FMOD caller does."""
    native.initialize(executable)
    uc = native.emulator()
    result = native.HEAP
    uc.mem_write(native.STOP + 32, b'\xd9\x1d' + struct.pack('<I', result))
    uc.mem_write(native.STOP + 16, b'\xdb\xe3')
    bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    rows = ['# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# 878320 -> 8782B0, installed by 87DDF3 through 8D1540',
            '# minimum maximum distance -> gain (f32 bits, hex)']
    for minimum, maximum in ((0., 0.), (0., 100.), (4., 4.), (4., 100.), (12.5, 100.), (50., 300.)):
        values = {0., minimum / 2., minimum, minimum * 2., maximum * .5,
                  maximum * .89, maximum * .9, maximum * .91, maximum * .95,
                  maximum, maximum + 1., maximum * 2., 1000.}
        for boundary in (minimum, maximum * .9, maximum):
            if boundary > 0.:
                for offset in (-1, 1):
                    values.add(struct.unpack('<f', struct.pack('<I', bits(boundary) + offset))[0])
        for distance in sorted(values):
            uc.emu_start(native.STOP + 16, native.STOP + 18, count=1)
            native.invoke(uc, 0x8782b0, [bits(minimum), bits(maximum), bits(distance)])
            uc.emu_start(native.STOP + 32, native.STOP + 38, count=1)
            gain = native.read_words(uc, result, 1)[0]
            rows.append(f'{bits(minimum):08x} {bits(maximum):08x} {bits(distance):08x} {gain:08x}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 3} original custom distance gains')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
