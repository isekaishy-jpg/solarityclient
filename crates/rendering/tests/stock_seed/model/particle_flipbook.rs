//! Native integer interpolation, including the street brazier's repeated midpoint.
use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::M2ParticleLifetimePose;

use super::{append_render_lifetime_track, render_m2_bytes, render_skin_bytes, render_u16_values};
use crate::support::{Fixture, FixtureFile};

/// Both particle heads and tails must advance through every native atlas frame.
#[test]
fn particle_flipbook_matches_native_lifetime_sampling() -> Result<(), Box<dyn Error>> {
    let samples = include_str!("../../fixtures/particle_flipbook_native.txt");
    let mut checked = 0;
    for name in ["single", "two", "descending", "three", "brazier", "uneven"] {
        let lines = samples
            .lines()
            .filter(|line| line.split_whitespace().next() == Some(name))
            .collect::<Vec<_>>();
        let fields = lines
            .first()
            .ok_or("missing native track")?
            .split_whitespace()
            .collect::<Vec<_>>();
        let timestamps = fields[1]
            .split(',')
            .map(str::parse)
            .collect::<Result<Vec<u16>, _>>()?;
        let values = fields[2]
            .split(',')
            .map(str::parse)
            .collect::<Result<Vec<u16>, _>>()?;
        let mut bytes = render_m2_bytes("Particle.blp", 1)?;
        let offset = u32::from_le_bytes(bytes[0x12c..0x130].try_into()?) as usize;
        for field in [0x13c, 0x14c] {
            append_render_lifetime_track(
                &mut bytes,
                offset + field,
                &timestamps,
                &render_u16_values(&values),
                2,
            )?;
        }
        let skin = render_skin_bytes()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "Creature/Solarity/Flipbook.m2",
                bytes: &bytes,
            },
            FixtureFile {
                path: "Creature/Solarity/Flipbook00.skin",
                bytes: &skin,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = DecodedM2Model::load(
            &mut store,
            &AssetPath::new("Creature/Solarity/Flipbook.m2")?,
        )?;
        let emitter = model.animations().particles().first().ok_or("emitter")?;
        for line in lines {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let age = f32::from_bits(u32::from_str_radix(fields[3], 16)?);
            let expected = fields[4].parse::<u32>()?;
            let pose = M2ParticleLifetimePose::sample(emitter, age, 0x1234)?;
            assert_eq!(pose.head_texture_cell(), expected, "head {line}");
            assert_eq!(pose.tail_texture_cell(), expected, "tail {line}");
            checked += 1;
        }
    }
    assert_eq!(checked, 792);
    Ok(())
}
