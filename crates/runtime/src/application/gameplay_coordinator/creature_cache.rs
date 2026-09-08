//! Creature template admission follows exact unit lifetimes across world replacement.

use std::collections::HashMap;
use std::rc::Rc;

use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_network::{CreatureQueryResponse, CreatureTemplate};

use super::template_cache::{TemplateBinding, TemplateCache};

struct UnitTemplate {
    entry: u32,
    binding: TemplateBinding<CreatureTemplate>,
}

pub(super) struct CreatureTemplateCache {
    cache: TemplateCache<CreatureTemplate>,
    units: HashMap<WorldObjectIdentity, UnitTemplate>,
}

impl CreatureTemplateCache {
    pub(super) fn new() -> Self {
        Self {
            cache: TemplateCache::new(),
            units: HashMap::new(),
        }
    }

    /// 72D940/72CEB0 bind entry/GUID callbacks. Unchanged units do not retry a
    /// missing reply; later unit admission may request that entry again.
    pub(super) fn synchronize_world(&mut self, world: Option<&ActiveWorld>) {
        let Some(world) = world else {
            self.units.clear();
            return;
        };
        self.units.retain(|identity, unit| {
            world.object_identity(identity.guid()) == Some(*identity)
                && world
                    .object_presentation(identity.guid())
                    .is_some_and(|presentation| presentation.entry_id() == unit.entry)
        });
        for identity in world.visible_units() {
            let Some(presentation) = world.object_presentation(identity.guid()) else {
                continue;
            };
            let entry = presentation.entry_id();
            if entry != 0
                && let std::collections::hash_map::Entry::Vacant(unit) = self.units.entry(identity)
            {
                let binding = self.cache.bind(entry, identity.guid());
                unit.insert(UnitTemplate { entry, binding });
            }
        }
    }

    pub(super) fn flags(&self, identity: WorldObjectIdentity) -> u32 {
        self.units
            .get(&identity)
            .and_then(|unit| unit.binding.template())
            .map_or(0, |template| template.flags())
    }

    pub(super) fn receive(&mut self, response: CreatureQueryResponse) {
        let entry = response.entry();
        let template = match response {
            CreatureQueryResponse::Found(template) => Some(Rc::from(template)),
            CreatureQueryResponse::Missing(_) => None,
        };
        self.cache.complete(entry, template);
    }

    pub(super) fn name(&self, identity: WorldObjectIdentity) -> Option<String> {
        let template = self.units.get(&identity)?.binding.template()?;
        let bytes = &template.strings()[0];
        (!bytes.is_empty()).then(|| String::from_utf8_lossy(bytes).into_owned())
    }

    pub(super) fn pending_request(&self) -> Option<(u32, u64)> {
        self.cache.pending_request()
    }

    pub(super) fn request_admitted(&mut self) {
        self.cache.request_admitted();
    }

    pub(super) fn clear(&mut self) {
        self.units.clear();
        self.cache.clear();
    }
}
