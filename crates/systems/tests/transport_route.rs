//! Compare complete transport construction and sampling with original machine code.

use glam::Vec3;
use solarity_systems::{
    TransportRoute, TransportRouteClock, TransportRouteEvent, TransportRouteMotion,
    TransportRouteNode, TransportRoutePhysics,
};

#[test]
fn routes_match_native_stops_events_maps_teleports_and_server_period_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut route: Option<TransportRoute> = None;
    let mut nodes = Vec::new();
    let mut speed = 0.0;
    let mut acceleration = 0.0;
    let mut expected_period = 0;
    let mut event_index = 0;
    let mut samples = 0;
    let mut routes = 0;
    let mut clock = TransportRouteClock::default();
    let mut clocks = 0;
    for line in include_str!("fixtures/transport-route-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields[0] {
            "clock_reset" => clock = TransportRouteClock::default(),
            "progress" => clock.synchronize_progress(
                route.as_ref().ok_or("progress before construction")?,
                fields[1].parse()?,
                fields[2].parse()?,
            ),
            "freeze" => clock.freeze_at_station(
                route.as_ref().ok_or("freeze before construction")?,
                fields[1].parse()?,
            ),
            "motion" => clock.set_motion(
                route.as_ref().ok_or("motion before construction")?,
                fields[1].parse()?,
                if fields[2] == "0" {
                    TransportRouteMotion::Moving
                } else {
                    TransportRouteMotion::StopAtStation
                },
            ),
            "clock" => {
                assert_eq!(
                    clock.clock_ms(
                        route.as_ref().ok_or("clock before construction")?,
                        fields[1].parse()?,
                        fields[2].parse()?
                    ),
                    fields[3].parse::<u32>()?,
                    "{line}"
                );
                clocks += 1;
            }
            "case" => {
                if let Some(route) = &route {
                    assert_eq!(event_index, route.events().len());
                }
                route = None;
                nodes.clear();
                event_index = 0;
                speed = float(fields[2])?;
                acceleration = float(fields[3])?;
                expected_period = fields[4].parse()?;
                routes += 1;
            }
            "node" => nodes.push(TransportRouteNode {
                map_id: fields[1].parse()?,
                position: Vec3::new(float(fields[2])?, float(fields[3])?, float(fields[4])?),
                flags: fields[5].parse()?,
                delay_seconds: fields[6].parse()?,
                arrival_event: fields[7].parse()?,
                departure_event: fields[8].parse()?,
            }),
            "section" => {
                if route.is_none() {
                    let constructed = TransportRoute::new(&nodes, speed, acceleration)?;
                    assert_eq!(constructed.period_ms(), expected_period, "{line}");
                    route = Some(constructed);
                }
            }
            // Retained oracle metadata helps inspect a timing regression without
            // exposing private construction tables through the production API.
            "stop" => {}
            "event" => {
                let route = route.as_ref().ok_or("event before construction")?;
                assert_eq!(
                    route.events().get(event_index),
                    Some(&TransportRouteEvent {
                        time_ms: fields[1].parse()?,
                        event_id: fields[2].parse()?,
                    }),
                    "{line}"
                );
                event_index += 1;
            }
            "period" => route
                .as_mut()
                .ok_or("period before construction")?
                .set_period_ms(fields[1].parse()?),
            "physics" => {
                let values = fields[1..]
                    .iter()
                    .map(|word| float(word))
                    .collect::<Result<Vec<_>, _>>()?;
                route
                    .as_mut()
                    .ok_or("physics before construction")?
                    .set_physics(Some(TransportRoutePhysics::new(
                        values
                            .try_into()
                            .map_err(|_| "invalid physics field count")?,
                    )?));
            }
            "sample" => {
                let route = route.as_ref().ok_or("sample before construction")?;
                let expected_map: u32 = fields[2].parse()?;
                let sample = route.sample(fields[1].parse()?);
                if expected_map == u32::MAX {
                    assert!(sample.is_none(), "{line}");
                } else {
                    let sample = sample.ok_or("missing native map placement")?;
                    assert_eq!(sample.map_id, expected_map, "{line}");
                    assert_eq!(sample.animation_id, fields[3].parse::<u32>()?, "{line}");
                    assert_eq!(sample.section_index, fields[4].parse::<usize>()?, "{line}");
                    for (actual, expected) in
                        sample.position.to_array().into_iter().zip(&fields[5..8])
                    {
                        close(actual, float(expected)?, line);
                    }
                    let angle_difference = (sample.yaw - float(fields[8])? + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    close(angle_difference, 0.0, line);
                    close(sample.roll, float(fields[9])?, line);
                    close(sample.pitch, float(fields[10])?, line);
                }
                samples += 1;
            }
            _ => return Err("unknown native fixture record".into()),
        }
    }
    assert_eq!(event_index, route.ok_or("no routes")?.events().len());
    assert_eq!(routes, 28);
    assert_eq!(samples, 6588);
    assert_eq!(clocks, 11200);
    Ok(())
}

fn float(word: &str) -> Result<f32, std::num::ParseIntError> {
    u32::from_str_radix(word, 16).map(f32::from_bits)
}

fn close(actual: f32, expected: f32, line: &str) {
    let tolerance = 0.00001_f32.max(expected.abs() * 0.000001);
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual}, native {expected}: {line}"
    );
}
