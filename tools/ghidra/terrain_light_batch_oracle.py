"""Capture original 2-by-2 terrain batch pairing and union light-query bounds.

Executes 7D6810, 7D66D0, 7C3B40 and 7B7AF0. Only the batch allocator is
substituted with bounded storage. No graphics or client entry point is run.
"""
import argparse
import random
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(weighted, layers):
    u = n.emulator()
    root, query = n.HEAP, n.HEAP+0x800
    chunks = [n.HEAP+0x2000+i*0x1000 for i in range(4)]
    for address, chunk in zip([0xbc,0xfc,0xc0,0x100], chunks):
        n.write_words(u, root+address, chunk)
    n.write_words(u, 0xcf08d0, weighted*4)
    for i, (chunk, material) in enumerate(zip(chunks, layers)):
        n.write_words(u, chunk+0x110, chunk+0x400)
        n.write_words(u, chunk+0x12c, chunk+0x500)
        n.write_words(u, chunk+0x40c, len(material))
        for j, (texture, flags) in enumerate(material):
            n.write_words(u, chunk+0x500+j*16, texture, flags, 0, 0)
        x, y = i//2, i%2
        n.write_floats(u, chunk+0x4c, [-32.*(x+1), -32.*(y+1), float(i),
                                     -32.*x, -32.*y, float(i+10)])
        n.write_floats(u, chunk+0x7c, [-32.*x, -32.*y, float(i)])
    next_owner = n.HEAP+0x18000
    def allocator(u, address, size, context):
        nonlocal next_owner
        if address == 0x7c0500:
            return_value(u, next_owner)
            next_owner += 0x100
    u.hook_add(UC_HOOK_CODE, allocator)
    u.reg_write(UC_X86_REG_ECX, root)
    n.invoke(u, 0x7d6810, [query])
    owners = [n.read_words(u, chunk+0xa8, 1)[0] for chunk in chunks]
    result = []
    for owner in owners:
        result += [owners.index(owner), *n.read_floats(u, owner+0x24, 4)]
    return result


def generate():
    rng = random.Random(12340)
    cases = []
    for weighted in range(2):
        for flags in [0,0x40,0x80,0x100,0x200,0x400,0x4c0]:
            for changed in range(4):
                layers = [[(0,0), (1,0)] for _ in range(4)]
                layers[changed] = [(0,flags),(1,0)]
                cases.append((weighted,layers))
        for _ in range(128):
            layers = [[(rng.randrange(6), rng.choice([0,0,0,0x40,0x80,0x100,0x200,0x400]))
                       for _ in range(rng.randrange(5))] for _ in range(4)]
            cases.append((weighted,layers))
        for textures in [(0,0,0,0),(0,0,1,1),(0,1,0,1),(0,1,2,3)]:
            cases.append((weighted,[[(value,0)] for value in textures]))
    rows = ['# weighted; four texture:flag lists (- means zero layers); four native (group,cx,cy,cz,radius) results']
    for weighted, layers in cases:
        encoded = [';'.join(f'{texture}:{flags}' for texture,flags in material) or '-' for material in layers]
        rows.append(' '.join([str(weighted),*encoded,*map(str,capture(weighted,layers))]))
    return '\n'.join(rows)+'\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(generate())
    print('Captured 320 original terrain batch pairings and query bounds')
