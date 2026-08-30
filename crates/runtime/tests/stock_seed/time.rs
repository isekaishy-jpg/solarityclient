//! External stock-compatibility tests for the authoritative realm clock.

use std::error::Error;
use std::time::Duration;

use solarity_network::WorldTimeSpeed;
use solarity_runtime::RealmClock;

/// The server rate advances minutes per real second and wraps each realm day.
#[test]
fn realm_clock_advances_stock_half_minutes() -> Result<(), Box<dyn Error>> {
    let source = WorldTimeSpeed::new((21 << 6) | 37, 1.0 / 60.0, 0x1122_3344)?;
    let clock = RealmClock::new(source);

    assert_eq!(clock.source(), source);
    assert_eq!(clock.half_minutes_after(Duration::ZERO), 2_594);
    assert_eq!(clock.half_minutes_after(Duration::from_secs(30)), 2_595);
    assert_eq!(clock.half_minutes_after(Duration::from_secs(8_580)), 0);
    Ok(())
}

/// A stopped realm clock remains authoritative rather than reading local time.
#[test]
fn zero_speed_realm_clock_does_not_advance() -> Result<(), Box<dyn Error>> {
    let clock = RealmClock::new(WorldTimeSpeed::new((3 << 6) | 5, 0.0, 0)?);

    assert_eq!(clock.half_minutes_after(Duration::from_secs(86_400)), 370);
    Ok(())
}

/// Invalid clocks and rates fail instead of installing a fallback.
#[test]
fn realm_clock_source_rejects_invalid_wire_values() {
    assert!(WorldTimeSpeed::new((24 << 6) | 5, 1.0, 0).is_err());
    assert!(WorldTimeSpeed::new((3 << 6) | 60, 1.0, 0).is_err());
    assert!(WorldTimeSpeed::new((3 << 6) | 5, -0.1, 0).is_err());
    assert!(WorldTimeSpeed::new((3 << 6) | 5, f32::NAN, 0).is_err());
}
