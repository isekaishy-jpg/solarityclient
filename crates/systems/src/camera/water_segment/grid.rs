//! Native liquid cell visitation. Coordinates here are grid-local X/Y.

use glam::Vec2;

const EPSILON: f64 = (f32::EPSILON * 2.0) as f64;

fn cell(value: f64) -> i32 {
    (value - 0.5).round_ties_even() as i32
}

pub(super) fn terrain_cells(start: Vec2, end: Vec2, output: &mut Vec<[u16; 2]>) {
    output.clear();
    let origin = f64::from(17_066.666_f32);
    let start_extended = glam::DVec2::splat(origin) - start.as_dvec2();
    let end_extended = glam::DVec2::splat(origin) - end.as_dvec2();
    let start = start_extended.as_vec2().as_dvec2();
    let end = end_extended.as_vec2().as_dvec2();
    let scale = f64::from(0.24_f32);
    let first = [
        cell(f64::from((start.x * scale) as f32)),
        cell(f64::from((start.y * scale) as f32)),
    ];
    // End Y retains its original extended subtraction for this conversion.
    let last = [
        cell(f64::from((end.x * scale) as f32)),
        cell(f64::from((end_extended.y * scale) as f32)),
    ];
    let delta = (end_extended - start_extended).as_vec2().as_dvec2();
    let axis = if delta.y.abs() < EPSILON || first[1] == last[1] {
        0
    } else if delta.x.abs() < EPSILON || first[0] == last[0] {
        1
    } else {
        usize::from(delta.y.abs() > delta.x.abs())
    };
    let other = 1 - axis;
    let mut current = first[axis];
    let step = if current <= last[axis] { 1 } else { -1 };
    let emit = |major: i32, minor: i32, output: &mut Vec<[u16; 2]>| {
        let mut pair = [0; 2];
        pair[axis] = major as u16;
        pair[other] = minor as u16;
        output.push(pair);
    };
    if delta[other].abs() < EPSILON || first[other] == last[other] {
        while current != last[axis] + step {
            emit(current, first[other], output);
            current += step;
        }
        return;
    }
    // 7A2230 walks world-Y columns; 7A23E0 walks world-X rows.
    let slope = (end.x - start.x) / (end.y - start.y);
    let intercept = (-start.y * slope + start.x) as f32;
    let coefficient = if axis == 0 {
        slope.recip() as f32
    } else {
        slope as f32
    };
    let spacing = f64::from(4.166_666_5_f32);
    let mut boundary = f64::from(current + i32::from(step > 0)) * spacing;
    let mut boundary_store = boundary as f32;
    let mut minor = first[other];
    emit(current, minor, output);
    while current != last[axis] + step {
        if output.len() >= 1022 {
            return;
        }
        let relative = if axis == 0 {
            (boundary - f64::from(intercept)) * f64::from(coefficient)
        } else {
            boundary
                * if current == first[axis] {
                    slope
                } else {
                    f64::from(coefficient)
                }
                + f64::from(intercept)
        };
        let next = cell(f64::from((relative * scale) as f32));
        if next != minor {
            emit(current, next, output);
        }
        current += step;
        emit(current, next, output);
        minor = next;
        boundary = f64::from(step) * spacing + f64::from(boundary_store);
        boundary_store = boundary as f32;
    }
    if minor != last[other] {
        emit(last[axis], last[other], output);
    }
}

pub(crate) fn wmo_cells(start: Vec2, end: Vec2, dimensions: [u32; 2], output: &mut Vec<[u16; 2]>) {
    output.clear();
    let start = start.as_dvec2();
    let end = end.as_dvec2();
    let delta = end - start;
    let axis = if delta.y.abs() < EPSILON {
        0
    } else if delta.x.abs() < EPSILON {
        1
    } else {
        usize::from(delta.x.abs() <= delta.y.abs())
    };
    let other = 1 - axis;
    let mut current = cell(start[axis]);
    let step = if current <= cell(end[axis]) { 1 } else { -1 };
    let stop = cell(end[axis]) + step;
    let emit = |major: i32, minor: i32, output: &mut Vec<[u16; 2]>| {
        let mut pair = [0; 2];
        pair[axis] = major as u8 as u16;
        pair[other] = minor as u8 as u16;
        output.push(pair);
    };
    if delta[other].abs() < EPSILON {
        let minor = cell(start[other]);
        while current != stop {
            emit(current, minor, output);
            current += step;
        }
        return;
    }
    let step = if start[axis] < end[axis] { 1 } else { -1 };
    let stop = cell(end[axis]) + step;
    let offset = if step == 1 {
        start[axis] - f64::from(current)
    } else {
        f64::from(current + 1) - start[axis]
    };
    let slope = delta[other] / delta[axis].abs();
    let intercept = (start[other] - slope * offset) as f32;
    let slope = slope as f32;
    let mut accumulated = 0.0_f32;
    let mut previous = cell(start[other]);
    let mut minor = previous;
    while current != stop {
        if minor != previous {
            emit(current, previous, output);
        }
        emit(current, minor, output);
        current += step;
        previous = minor;
        let next = f64::from(accumulated) + f64::from(slope);
        accumulated = next as f32;
        minor = cell(f64::from((next + f64::from(intercept)) as f32));
    }
    if minor != previous && minor >= 0 && minor < dimensions[other] as i32 {
        emit(current - step, minor, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_grid_visitation_matches_original() -> Result<(), Box<dyn std::error::Error>> {
        let mut actual = Vec::new();
        for line in include_str!("../../../tests/fixtures/camera-water-grid-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let groups = line
                .split('|')
                .map(|group| {
                    group
                        .split_whitespace()
                        .map(|word| u32::from_str_radix(word, 16))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let v = &groups[0];
            wmo_cells(
                Vec2::new(f32::from_bits(v[0]), f32::from_bits(v[1])),
                Vec2::new(f32::from_bits(v[2]), f32::from_bits(v[3])),
                [16, 16],
                &mut actual,
            );
            let expected: Vec<_> = groups[1]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| pair.map(|v| v as u16))
                .collect();
            assert_eq!(actual, expected, "{line}");
        }
        Ok(())
    }

    #[test]
    fn terrain_water_grid_visitation_matches_original() -> Result<(), Box<dyn std::error::Error>> {
        let mut actual = Vec::new();
        for line in include_str!("../../../tests/fixtures/camera-terrain-water-grid-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let groups = line
                .split('|')
                .map(|group| {
                    group
                        .split_whitespace()
                        .map(|word| u32::from_str_radix(word, 16))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let v = &groups[0];
            terrain_cells(
                Vec2::new(f32::from_bits(v[0]), f32::from_bits(v[1])),
                Vec2::new(f32::from_bits(v[2]), f32::from_bits(v[3])),
                &mut actual,
            );
            let expected: Vec<_> = groups[1]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| [pair[1] as u16, pair[0] as u16])
                .collect();
            assert_eq!(actual, expected, "{line}");
        }
        Ok(())
    }
}
