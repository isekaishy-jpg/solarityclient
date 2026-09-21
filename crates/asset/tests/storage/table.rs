use super::*;
use std::error::Error;

#[test]
fn cache_table_bound_covers_pinned_hashbrown_layouts() -> Result<(), Box<dyn Error>> {
    #[repr(align(64))]
    #[derive(Eq, Hash, PartialEq)]
    struct Aligned(u8);
    fn check<K: Eq + Hash, V>() -> Result<(), Box<dyn Error>> {
        for capacity in 1..=256 {
            let mut map: Map<K, V> = Map::with_hasher(RandomState::new());
            map.try_reserve(capacity)?;
            assert!(table_bound::<(K, V)>(capacity)? >= map.allocation_size());
        }
        Ok(())
    }
    check::<(), ()>()?;
    check::<u8, u8>()?;
    check::<u64, Vec<u8>>()?;
    check::<Aligned, Aligned>()?;
    Ok(())
}
