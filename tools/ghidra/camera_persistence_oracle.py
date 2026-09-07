"""Capture original saved-camera restore stores and ordinary save arithmetic."""
from pathlib import Path
import struct
import argparse
import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP

def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    camera, distance, pitch = native.HEAP, native.HEAP + 0x1000, native.HEAP + 0x1100
    native.write_words(uc, 0xc24e7c, distance)
    native.write_words(uc, 0xc24e74, pitch)
    f32bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    rows=['# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
          '# restore: saved distance, saved pitch degrees -> distance, pitch radians (f32 bits)',
          '# save: ordinary current distance, pitch radians -> formatter distance, pitch degrees (f64 bits)']
    for saved_distance in [-1., 0., .1, 5.55, 15., 49.999, 50., 75.]:
        for saved_pitch in [-180., -45., 0., 10., 80., 180.]:
            native.write_floats(uc, distance + 0x2c, [saved_distance])
            native.write_floats(uc, pitch + 0x2c, [saved_pitch])
            uc.reg_write(UC_X86_REG_ECX, camera)
            native.invoke(uc, 0x5ff3e0, [])
            restored_distance = native.read_words(uc, camera + 0x118, 1)[0]
            restored_pitch = native.read_words(uc, camera + 0x120, 1)[0]
            assert native.read_words(uc, camera + 0x1e8, 1)[0] == restored_distance
            assert native.read_words(uc, camera + 0x230, 1)[0] == restored_pitch
            rows.append(f'restore {f32bits(saved_distance):08x} {f32bits(saved_pitch):08x} {restored_distance:08x} {restored_pitch:08x}')

    arguments=[]
    def hook(uc, address, size, user):
        if address not in (0x76f070, 0x7668c0):
            return
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address == 0x76f070:
            # Observe the completed arithmetic at the external formatter boundary.
            arguments.append(struct.unpack('<Q',uc.mem_read(sp+16,8))[0])
            cleanup=4
        else:
            # The CVar setter owns strings/storage; no camera arithmetic is replaced.
            cleanup=24
        uc.reg_write(UC_X86_REG_EIP,native.read_words(uc,sp,1)[0])
        uc.reg_write(UC_X86_REG_ESP,sp+cleanup)
    uc.hook_add(UC_HOOK_CODE,hook)
    for current_distance in [0., 5.55, 12.345678, 50.]:
        for current_pitch in [-1.5, -.25, 0., .17453292, .888888, 1.4]:
            arguments.clear()
            native.write_words(uc,camera+0x98,0,0)
            native.write_words(uc,camera+0x2c4,0)
            native.write_floats(uc,camera+0x1e8,[current_distance])
            native.write_floats(uc,camera+0x230,[current_pitch])
            uc.reg_write(UC_X86_REG_ECX,camera)
            native.invoke(uc,0x5ff320,[])
            assert len(arguments)==2
            rows.append(f'save {f32bits(current_distance):08x} {f32bits(current_pitch):08x} {arguments[0]:016x} {arguments[1]:016x}')
    output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'Captured {len(rows)-3} native camera persistence cases')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    capture(args.executable, args.output)


if __name__ == '__main__':
    main()
