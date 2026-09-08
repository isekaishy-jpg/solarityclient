//! External stock-compatibility tests for the authoritative realm clock.

use std::error::Error;
use std::time::Duration;

use solarity_network::WorldTimeSpeed;
use solarity_runtime::{RealmClock, world_model_environment_emissive};

/// The server rate advances minutes per real second and wraps each realm day.
#[test]
fn realm_clock_advances_stock_half_minutes() -> Result<(), Box<dyn Error>> {
    let source = WorldTimeSpeed::new((21 << 6) | 37, 1.0 / 60.0, 0x1122_3344)?;
    let clock = RealmClock::new(source);

    assert_eq!(clock.source(), source);
    assert_eq!(clock.half_minutes_after(Duration::ZERO), 2_594);
    assert_eq!(clock.half_minutes_after(Duration::from_secs(30)), 2_595);
    assert_eq!(clock.half_minutes_after(Duration::from_secs(8_580)), 0);
    assert_eq!(clock.day_milliseconds_after(Duration::ZERO), 77_820_000);
    assert_eq!(
        clock.day_milliseconds_after(Duration::from_secs(30)),
        77_850_000
    );
    assert_eq!(clock.day_milliseconds_after(Duration::from_secs(8_580)), 0);
    Ok(())
}

/// MapObj additive color follows stock's one-hour dawn and dusk ramps.
#[test]
fn world_model_emissive_uses_stock_daynight_ramps() {
    assert_eq!(world_model_environment_emissive(0), 1.0);
    assert_eq!(world_model_environment_emissive(6 * 120), 1.0);
    assert_eq!(world_model_environment_emissive(6 * 120 + 60), 0.5);
    assert_eq!(world_model_environment_emissive(7 * 120), 0.0);
    assert_eq!(world_model_environment_emissive(20 * 120 + 60), 0.0);
    assert_eq!(world_model_environment_emissive(21 * 120), 0.5);
    assert_eq!(world_model_environment_emissive(21 * 120 + 60), 1.0);
    assert_eq!(world_model_environment_emissive(24 * 120), 1.0);
}

/// A stopped realm clock remains authoritative rather than reading local time.
#[test]
fn zero_speed_realm_clock_does_not_advance() -> Result<(), Box<dyn Error>> {
    let clock = RealmClock::new(WorldTimeSpeed::new((3 << 6) | 5, 0.0, 0)?);

    assert_eq!(clock.half_minutes_after(Duration::from_secs(86_400)), 370);
    assert_eq!(
        clock.day_milliseconds_after(Duration::from_secs(86_400)),
        11_100_000
    );
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

/// Native sky time retains millisecond precision and maps the same sample to sunlight.
#[test]
fn realm_sky_clock_matches_original_millisecond_samples() -> Result<(), Box<dyn Error>> {
    let decode = |raw: &str| -> Result<Vec<f32>, Box<dyn Error>> {
        let bytes = raw
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
            .collect())
    };
    let mut count = 0;
    for line in include_str!("../fixtures/realm_sky_clock_native.txt")
        .lines()
        .filter(|l| l.starts_with("clock "))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        let minute: u32 = p[1].parse()?;
        let rate = f32::from_bits(u32::from_str_radix(p[2], 16)?);
        let source = WorldTimeSpeed::new((minute / 60) << 6 | (minute % 60), rate, 0)?;
        let clock = RealmClock::new(source);
        let elapsed = Duration::from_millis(p[3].parse()?);
        let sample = clock.sky_time_after(elapsed).ok_or("native calendar")?;
        assert_eq!(
            sample.day_fraction(),
            decode(p[4])?[0],
            "native sky clock {line}"
        );
        let actual = solarity_asset::exterior_light_direction_at(sample.day_fraction());
        for (a, b) in actual.to_array().into_iter().zip(decode(p[5])?) {
            assert!(
                (a + b).abs() < 0.000_002,
                "native exterior direction {line}: {actual}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 330);
    Ok(())
}

/// The realm date advances across leap day, and fixed map time resets lunar phase.
#[test]
fn realm_sky_calendar_rolls_and_honors_map_overrides() -> Result<(), Box<dyn Error>> {
    let packet = |year: u32, month: u32, day: u32| {
        WorldTimeSpeed::new(
            (year - 2000) << 24 | (month - 1) << 20 | (day - 1) << 14,
            1. / 60.,
            0,
        )
    };
    let clock = RealmClock::new(packet(2024, 2, 28)?);
    let initial = clock
        .sky_time_after(Duration::ZERO)
        .ok_or("initial calendar")?;
    for (days, month, day) in [(1, 2, 29), (2, 3, 1), (3, 3, 2)] {
        let advanced = clock
            .sky_time_after(Duration::from_secs(days * 86400))
            .ok_or("advanced calendar")?;
        let reference = RealmClock::new(packet(2024, month, day)?)
            .sky_time_after(Duration::ZERO)
            .ok_or("reference calendar")?;
        assert_eq!(advanced.calendar_days(), reference.calendar_days());
        assert_eq!(
            advanced.calendar_days(),
            initial.calendar_days() + days as i32
        );
    }
    for (minutes, expected) in [
        (0, 0.),
        (360, 0.25),
        (720, 0.5),
        (1440, 1.),
        (-5, 0.5),
        (1500, 0.5),
    ] {
        let overridden = initial.with_map_time_override(minutes);
        assert_eq!(overridden.day_fraction(), expected);
        assert_eq!(overridden.calendar_days(), 0);
    }
    Ok(())
}
