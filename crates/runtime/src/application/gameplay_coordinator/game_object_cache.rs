//! GameObject-specific completion of the shared native template cache.

use super::template_cache::{TemplateBinding, TemplateCache};
use solarity_network::{GameObjectQueryResponse, GameObjectTemplate};
use std::rc::Rc;

pub(in crate::application) type GameObjectTemplateBinding = TemplateBinding<GameObjectTemplate>;
pub(in crate::application) type GameObjectTemplateCache = TemplateCache<GameObjectTemplate>;

impl TemplateCache<GameObjectTemplate> {
    pub(in crate::application) fn receive(&mut self, response: GameObjectQueryResponse) {
        let entry = response.entry();
        let template = match response {
            GameObjectQueryResponse::Found(template) => Some(Rc::from(template)),
            GameObjectQueryResponse::Missing(_) => None,
        };
        self.complete(entry, template);
    }
}
