//! Explicit offline player appearances; Soap is the saved performance fixture.

use solarity_asset::{ArchiveCatalog, AssetStore, CharacterRaceCatalog};
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, PlayerEquipment, PlayerMoney,
    PlayerProgression, UnitAnimationTier, UnitFlags, UnitIdentity, UnitPresentation,
    UnitSheathState, UnitStats, UnitVitals, VisibleEquipmentItem,
};
use solarity_runtime::RuntimeConfiguration;
use std::error::Error;

#[derive(Clone, Copy)]
pub(super) enum CharacterFixture {
    Human,
    Soap,
}

impl CharacterFixture {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Human => "WorldBenchmark",
            Self::Soap => "Soap",
        }
    }

    pub(super) fn publish(
        self,
        world: &mut ActiveWorld,
        configuration: &RuntimeConfiguration,
    ) -> Result<(), Box<dyn Error>> {
        let (identity, display, appearance, equipment) = match self {
            Self::Human => (
                UnitIdentity::new(1, 1, 0, 1, 1, 1),
                49,
                PlayerAppearance::default(),
                PlayerEquipment::default(),
            ),
            Self::Soap => {
                let mut store = AssetStore::mount(ArchiveCatalog::discover(
                    configuration.data_root().clone(),
                    configuration.locale(),
                )?)?;
                let races = CharacterRaceCatalog::load(&mut store)?;
                let race = races.race(10).ok_or("missing blood elf fixture race")?;
                // Saved Soap diagnostic: female blood elf mage, level 80. These
                // are explicit fixture facts, not a substitute for live login.
                let items = [
                    51281, 54581, 51284, 0, 51283, 50702, 51282, 50699, 54582, 51280, 54576, 50664,
                    54588, 50365, 54583, 0, 50719, 50684, 51534,
                ];
                (
                    UnitIdentity::new(10, 8, 1, 0, 80, race.faction_id()),
                    race.female_display_id(),
                    PlayerAppearance::new(6, 3, 0, 2, 5),
                    PlayerEquipment::new(items.map(|id| VisibleEquipmentItem::new(id, 0))),
                )
            }
        };
        let player = world.local_player();
        world.storage_mut().add_component(
            player,
            (
                ObjectKind::Player,
                ObjectPresentation::new(0, 1.),
                identity,
                UnitPresentation::new(
                    display,
                    display,
                    0,
                    0,
                    UnitAnimationTier::Ground,
                    UnitSheathState::Unarmed,
                ),
                UnitFlags::default(),
                appearance,
                equipment,
            ),
        );
        world.storage_mut().add_component(
            player,
            (
                PlayerMoney::new(0),
                PlayerProgression::new(0, 400),
                UnitVitals::new(100, 100, [0; 7], [100; 7]),
                UnitStats::new([20; 5], [0; 5], [0; 5]),
            ),
        );
        Ok(())
    }
}
