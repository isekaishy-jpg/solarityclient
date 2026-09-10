"""Run 004E3620's empty-character fallback against the pinned build-12340 image.

Only Lua argument/result adapters, numeric conversion, and the streaming-trial
query are stubbed. Original bounds checks and ChrRaces row-2 lookup execute.
"""
import argparse
import struct
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(index, file_string):
    u = n.emulator()
    table, row, string = n.HEAP, n.HEAP + 0x100, n.HEAP + 0x300
    n.write_words(u, 0xad3438, 2)
    n.write_words(u, 0xad3434, 2)
    n.write_words(u, 0xad3448, table)
    n.write_words(u, table, row)
    n.write_words(u, row + 0x2c, string)
    u.mem_write(string, file_string.encode() + b'\0')
    n.write_words(u, 0xb6b23c, 0)
    result = []

    def adapter(uc, address, size, data):
        stack = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x84df20:
            uc.reg_write(UC_X86_REG_EAX, 1)
        elif address == 0x88b9c0:
            uc.reg_write(UC_X86_REG_EAX, index)
        elif address == 0x422140:
            uc.reg_write(UC_X86_REG_EAX, 0)
        elif address == 0x84e350:
            pointer = struct.unpack('<I', uc.mem_read(stack + 8, 4))[0]
            result.append(bytes(uc.mem_read(pointer, 128)).split(b'\0')[0].decode())
        uc.reg_write(UC_X86_REG_EIP, struct.unpack('<I', uc.mem_read(stack, 4))[0])
        uc.reg_write(UC_X86_REG_ESP, stack + 4)

    for address in [0x84df20, 0x84e030, 0x88b9c0, 0x422140, 0x84e350]:
        u.hook_add(UC_HOOK_CODE, adapter, begin=address, end=address)
    n.invoke(u, 0x4e3620, [n.HEAP + 0x400])
    assert result == [file_string], result
    return result[0]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    args = parser.parse_args()
    n.initialize(args.executable)
    for index in [0, 1, 999]:
        for token in ['Orc', 'AuthoredRaceTwo']:
            print(index, token, capture(index, token))
