
The engine cannot be exchanged between output owners:

```compile_fail
use solarity_media::OwnedSoundEngine;

fn exchange(first: &mut OwnedSoundEngine, second: &mut OwnedSoundEngine) {
    std::mem::swap(&mut **first, &mut **second);
}
```

Scoped access also rejects exchanging engines between two callbacks:

```compile_fail
use solarity_media::OwnedSoundEngine;

fn exchange(first: &mut OwnedSoundEngine, second: &mut OwnedSoundEngine) {
    first.with_engine_mut(|left| {
        second.with_engine_mut(|right| std::mem::swap(left, right));
    });
}
```

An engine reference cannot escape its owner's callback:

```compile_fail
use solarity_media::{OwnedSoundEngine, SoundEngine};

fn escape(owner: &mut OwnedSoundEngine) -> &mut SoundEngine<'static> {
    owner.with_engine_mut(|engine| engine)
}
```

A callback cannot install an engine borrowing a local output:

```compile_fail
use solarity_media::{OwnedSoundEngine, SoundEngine};

fn replace(owner: &mut OwnedSoundEngine, replacement: SoundEngine<'_>) {
    owner.with_engine_mut(|engine| {
        *engine = replacement;
    });
}
```
