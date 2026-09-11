"""Probe the original mount/body authored adapter and breath factory binding.

734A40, 732650 and 6F9260 execute from the fingerprinted build-12340 image.
GUID lookup, finite-position validation, model readiness/attachment queries,
effect allocation and model construction are provider boundaries. This probes
which model the factory queries, not the supplied model's bone calculation,
CEffect rendering, constructor RNG, or the mount completion callback.
"""
import argparse
import itertools
import json
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as n
from unit_water_effect_oracle import Oracle, returned


class BindingOracle(Oracle):
    def __init__(self):
        super().__init__()
        self.body, self.mount, self.definition = [n.HEAP + offset for offset in (0x8000, 0x9000, 0xa000)]
        self.queries = []
        self.constructed = 0
        self.mode = 0
        n.write_words(self.uc, self.fields, 1, 2, 8)
        n.write_words(self.uc, self.unit + 0xb4, self.body)
        n.write_words(self.uc, self.unit + 0x98c, self.mount)
        n.write_words(self.uc, self.unit + 0x970, self.model)
        n.write_words(self.uc, self.definition + 8, self.definition + 0x100)
        n.write_floats(self.uc, self.foot, [3., 4., 5.])

    def hook(self, uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d4db0:
            assert n.read_words(uc, sp + 4, 3) == (1, 2, 8)
            returned(uc, self.unit if self.live else 0)
        elif address == 0x40c947:
            returned(uc, 0)  # Inputs are finite; invalid-coordinate logging is excluded.
        elif address == 0x732650:
            assert uc.reg_read(UC_X86_REG_ECX) == self.unit
            assert n.read_words(uc, sp + 4, 5) == (91, 0x48544224, 7, self.foot, 21)
        elif address == 0x6f9260:
            self.kind = n.read_words(uc, sp + 4, 1)[0]
            # Run the factory itself rather than the older decision-only hook.
        elif address == 0x824f00:
            self.queries.append(('ready', uc.reg_read(UC_X86_REG_ECX), 0))
            returned(uc, 1, 8)
        elif address == 0x8273d0:
            model = uc.reg_read(UC_X86_REG_ECX)
            attachment = n.read_words(uc, sp + 4, 1)[0]
            self.queries.append(('attachment', model, attachment))
            present = self.body_has_17 if model == self.body else self.mount_has_17
            returned(uc, int(present), 4)
        elif address == 0x65c290:
            returned(uc, self.definition, 4)
        elif address == 0x6f8a60:
            returned(uc, pop=24)  # No prior matching CEffect in this provider.
        elif address == 0x81f8f0:
            self.constructed += 1
            returned(uc, 0, 8)  # Record construction; no new model/load callback runs.
        elif address == 0x6f76c0:
            returned(uc, pop=4)
        else:
            super().hook(uc, address, size, context)

    def probe(self, emitter, body_has_17, mount_has_17, state, live):
        self.body_has_17, self.mount_has_17, self.live = body_has_17, mount_has_17, live
        self.queries.clear()
        self.constructed, self.kind = 0, -1
        self.uc.mem_write(self.effect, bytes(0x110))
        n.write_words(self.uc, self.unit + 0xa30, state)
        n.invoke(self.uc, 0x734a40, [self.mount if emitter else self.body,
                                   91, 0x48544224, 7, self.foot, 21, 1, 2])
        if live:
            assert self.queries == [('ready', self.body, 0), ('attachment', self.body, 17)]
            assert self.constructed == 1
            attachment = n.read_words(self.uc, self.effect + 0x24, 1)[0]
            assert attachment == (17 if body_has_17 else 19)
        else:
            assert self.queries == [] and self.constructed == 0
            attachment = -1
        return dict(emitter=emitter, body_has_17=body_has_17, mount_has_17=mount_has_17,
                    state=state, live=live, effect_kind=self.kind, attachment=attachment,
                    queried_body=bool(self.queries), constructions=self.constructed)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    oracle = BindingOracle()
    rows = [oracle.probe(*inputs) for inputs in itertools.product(
        range(2), range(2), range(2), [0x20, 0x40, 0x60], range(2))]
    args.output.write_text(json.dumps(rows, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(dict(records=len(rows), body_queries=sum(row['queried_body'] for row in rows))))
