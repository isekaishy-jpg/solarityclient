//! External stock-compatibility tests for Visual C++ `rand` ownership.

use solarity_runtime::{BlizzardRand, CrtRand};

/// A new build-12340 client thread begins at the CRT seed-one sequence.
#[test]
fn crt_rand_reproduces_stock_thread_sequence() {
    let mut random = CrtRand::new();
    let values = std::array::from_fn::<_, 10, _>(|_index| random.next_u15());

    assert_eq!(
        values,
        [
            41, 18_467, 6_334, 26_500, 19_169, 15_724, 11_478, 29_358, 26_962, 24_464,
        ]
    );
}

/// The process-wide Blizzard table generator matches build 12340's words.
#[test]
fn blizzard_rand_reproduces_stock_table_sequence() {
    let mut random = BlizzardRand::new(0x1234_5678);
    assert_eq!(random.next_u32(), 0x28ea_c7b8);
    assert_eq!(random.next_u32(), 0xccbc_2deb);
    assert_eq!(random.next_u32(), 0x2889_ad69);
    assert_eq!(random.next_u32(), 0x0e71_8081);
}
