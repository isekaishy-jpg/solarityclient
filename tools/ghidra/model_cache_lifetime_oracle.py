"""Execute stock M2 release, reacquisition and timed collection in isolation.

Runs build-12340 83DC90, 835970 and 81C290, plus the 81C698 lookup-insertion
block through 81C6CA. The clock, destructor and allocator
free are supplied boundaries; reference counts, queue links and age comparisons
execute original instructions. This does not run resource construction, GPU
completion or archive loading. The insertion probes establish the local flag-8
hash-membership gate, not the lookup flags selected by every gameplay caller.
"""
import argparse
import json
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EIP,
    UC_X86_REG_ESI, UC_X86_REG_ESP,
)

import wmo_registration_oracle as native


def insert_lookup(uc, resource, cache, flags, successor):
    """Execute only the recovered insertion block with an owned bucket boundary."""
    frame, bucket = native.STACK + 0x10000, cache + 0x10
    existing = native.HEAP + 0x1000 if successor else 0
    native.write_words(uc, bucket, existing)
    if existing:
        native.write_words(uc, existing + 0x144, bucket, 0)
    native.write_words(uc, frame - 4, bucket)
    native.write_words(uc, frame + 0xc, flags)
    uc.reg_write(UC_X86_REG_EBP, frame)
    uc.reg_write(UC_X86_REG_ESI, resource)
    uc.emu_start(0x81c698, 0x81c6ca, timeout=1_000_000, count=100)
    assert uc.reg_read(UC_X86_REG_EIP) == 0x81c6ca
    inserted = not bool(flags & 8)
    assert native.read_words(uc, bucket, 1)[0] == (resource if inserted else existing)
    assert native.read_words(uc, resource + 0x144, 2) == (
        (bucket, existing) if inserted else (0, 0)
    )
    if existing:
        assert native.read_words(uc, existing + 0x144, 1)[0] == (
            resource + 0x148 if inserted else bucket
        )
    assert native.read_words(uc, resource + 8, 1)[0] == (flags & 0x40)
    return inserted


def probe(created, age, reacquire, force, cacheable=True, references=1,
          cache_owner=True, lookup_flags=None, successor=False):
    """Check one release/collection boundary with a controlled millisecond clock."""
    uc = native.emulator()
    resource, cache = native.HEAP, native.HEAP + 0x2000
    now = [created]
    destroyed, freed = [], []

    def providers(machine, address, _size, _context):
        """Replace only OS time and terminal destruction side effects."""
        argument_bytes = 0
        if address == 0x86ae20:
            machine.reg_write(UC_X86_REG_EAX, now[0])
        elif address == 0x83d5b0:
            destroyed.append(machine.reg_read(UC_X86_REG_ECX))
        elif address == 0x76e5a0:
            sp = machine.reg_read(UC_X86_REG_ESP)
            freed.append(native.read_words(machine, sp + 4, 1)[0])
            argument_bytes = 16  # SMemFree's four arguments are callee-cleaned.
        else:
            return
        sp = machine.reg_read(UC_X86_REG_ESP)
        machine.reg_write(UC_X86_REG_EIP, native.read_words(machine, sp, 1)[0])
        machine.reg_write(UC_X86_REG_ESP, sp + 4 + argument_bytes)

    uc.hook_add(UC_HOOK_CODE, providers)
    native.write_words(uc, resource, references, cache if cache_owner else 0)
    if lookup_flags is None:
        native.write_words(uc, resource + 0x144, int(cacheable))
    else:
        cacheable = insert_lookup(uc, resource, cache, lookup_flags, successor)
    native.write_words(uc, cache + 8, 0, cache + 8)
    uc.reg_write(UC_X86_REG_ECX, resource)
    native.invoke(uc, 0x83dc90, [])
    released_to_queue = native.read_words(uc, cache + 8, 1)[0] == resource
    qualified = cache_owner and cacheable
    assert released_to_queue == (qualified and references == 1)
    if reacquire and released_to_queue:
        uc.reg_write(UC_X86_REG_ECX, resource)
        native.invoke(uc, 0x835970, [])
        assert native.read_words(uc, resource, 1)[0] == 1
        assert native.read_words(uc, cache + 8, 2) == (0, cache + 8)
    now[0] = (created + age) & 0xffffffff
    uc.reg_write(UC_X86_REG_ECX, cache)
    native.invoke(uc, 0x81c290, [int(force)])
    signed_age = ((age + 0x80000000) & 0xffffffff) - 0x80000000
    expected = references == 1 and (
        not qualified or (not reacquire and (force or signed_age >= 10000))
    )
    assert destroyed == ([resource] if expected else [])
    assert freed == destroyed
    return {
        "created_ms": created, "age_ms": age, "reacquired": reacquire,
        "forced": force, "cacheable": cacheable, "initial_references": references,
        "queued_on_release": released_to_queue, "destroyed": bool(destroyed),
        "cache_owner": cache_owner, "lookup_flags": lookup_flags,
        "hash_successor": successor,
    }


def capture(executable, output):
    """Verify the binary fingerprint and write reproducible native outcomes."""
    native.initialize(executable)
    rows = [
        probe(created, age, reacquire, force)
        for created in (1000, 0xfffffff0)
        for age in (0, 9999, 10000, 10001)
        for reacquire in (False, True)
        for force in (False, True)
    ]
    rows.extend(probe(1000, 10001, False, False, cacheable, references)
                for cacheable in (False, True) for references in (1, 2))
    rows.extend(probe(1000, age, reacquire, False, lookup_flags=flags,
                      successor=successor)
                for flags in (0, 8, 0x40, 0x48)
                for successor in (False, True)
                for age in (0, 9999, 10000)
                for reacquire in (False, True))
    rows.extend(probe(1000, 0, False, False, cacheable, references, cache_owner=False)
                for cacheable in (False, True) for references in (1, 2))
    rows.extend(probe(created, age, reacquire, force)
                for created in (1000, 0xfffffff0)
                for age in (0x7fffffff, 0x80000000, 0xffffffff)
                for reacquire in (False, True) for force in (False, True))
    Path(output).write_text(json.dumps({"cases": rows}, indent=2) + "\n", encoding="utf-8")
    print(f"verified {len(rows)} stock M2 lifetime cases")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable")
    parser.add_argument("output")
    args = parser.parse_args()
    capture(args.executable, args.output)
