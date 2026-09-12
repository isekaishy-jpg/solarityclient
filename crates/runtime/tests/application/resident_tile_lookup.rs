//! Direct lookup retains the old ordered search across every possible map key.

use super::*;

#[test]
fn resident_tile_lookup_tracks_reordering_eviction_and_reuse()
-> Result<(), Box<dyn std::error::Error>> {
    let all = (0..64)
        .flat_map(|y| (0..64).map(move |x| TerrainTileIndex::new(x, y)))
        .collect::<Option<Vec<_>>>()
        .ok_or("map key")?;
    let mut lookup = ResidentTileLookup::default();
    let compare = |lookup: &ResidentTileLookup, resident: &[TerrainTileIndex]| {
        for &tile in &all {
            assert_eq!(
                lookup.get(tile),
                resident.iter().position(|&entry| entry == tile),
                "{tile:?}"
            );
        }
    };
    compare(&lookup, &[]);
    let mut resident = Vec::new();
    // Include both outer map edges and the largest representable resident slot.
    for &tile in all.iter().rev() {
        lookup.insert(tile, resident.len());
        resident.push(tile);
    }
    compare(&lookup, &resident);
    resident.retain(|tile| tile.x() % 3 == 0 && tile.y() % 5 == 0);
    lookup.rebuild(resident.iter().copied());
    compare(&lookup, &resident);
    // Promotion removes an arbitrary neighbor; the previous primary appends.
    for slot in [0, 15, 100] {
        let primary = resident.remove(slot);
        resident.push(primary);
        lookup.rebuild(resident.iter().copied());
        compare(&lookup, &resident);
    }
    lookup.rebuild(std::iter::empty());
    compare(&lookup, &[]);
    lookup.insert(all[0], 0);
    compare(&lookup, &all[..1]);
    Ok(())
}
