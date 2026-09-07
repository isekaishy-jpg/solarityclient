//! Native 7D9470 edge clipping and 7D9400 alternating strip traversal.

/// One polygon vertex with the overloaded MLIQ attributes kept before conversion.
#[derive(Clone, Copy)]
pub(super) struct ClipVertex {
    pub position: [f32; 3],
    /// Water uses the low byte; magma interpolation treats both words unsigned.
    pub attributes: [u16; 2],
}

/// Edge links stay stable when a plane disables an edge or adds intersections.
struct Node {
    vertex: ClipVertex,
    previous: usize,
    next: usize,
}

/// Directed polygon boundary, retaining disabled slots as native code does.
struct Edge {
    start: usize,
    end: usize,
    enabled: bool,
}

/// An authored liquid cell clipped against zero or more neighboring portals.
pub(super) struct Polygon {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl Polygon {
    /// 7D9230/7D9270 initializes the TL, BL, BR, TR boundary ring.
    pub fn new(vertices: [ClipVertex; 4]) -> Self {
        Self {
            nodes: vertices
                .into_iter()
                .enumerate()
                .map(|(index, vertex)| Node {
                    vertex,
                    previous: (index + 3) % 4,
                    next: index,
                })
                .collect(),
            edges: (0..4)
                .map(|index| Edge {
                    start: index,
                    end: (index + 1) % 4,
                    enabled: true,
                })
                .collect(),
        }
    }

    /// Retains the signed positive half-plane with native intersection ordering.
    pub fn clip(&mut self, plane: [f32; 4], side: i16, authored_uv: bool) {
        let initial_nodes = self.nodes.len();
        let crossing_edge = self.edges.len();
        let mut crossing_start = None;
        let mut crossing_end = None;
        let mut crossings = Vec::with_capacity(2);
        for (edge_index, edge) in self.edges.iter_mut().enumerate() {
            if !edge.enabled {
                continue;
            }
            let start = self.nodes[edge.start].vertex;
            let end = self.nodes[edge.end].vertex;
            let a = distance(start.position, plane, side);
            let b = distance(end.position, plane, side);
            if a > 0.0 && b > 0.0 {
                continue;
            }
            if a < 0.0 && b < 0.0 {
                edge.enabled = false;
                continue;
            }
            let total = f64::from(a).abs() + f64::from(b).abs();
            if total < f64::from(f32::EPSILON) {
                continue;
            }
            // 7D9470 retains the x87 fraction for positions, but spills it to
            // float before the recursive 7A7E50/7A7F00 attribute callback.
            let fraction = f64::from(a).abs() / total;
            let vertex = interpolate(start, end, fraction, authored_uv);
            let index = self.nodes.len();
            if a >= 0.0 {
                self.nodes.push(Node {
                    vertex,
                    previous: edge_index,
                    next: crossing_edge,
                });
                crossing_start = Some(index);
            } else {
                self.nodes.push(Node {
                    vertex,
                    previous: crossing_edge,
                    next: edge_index,
                });
                crossing_end = Some(index);
            }
            crossings.push((edge_index, index, a >= 0.0));
        }
        if let (2, Some(start), Some(end)) = (crossings.len(), crossing_start, crossing_end) {
            for (edge, index, exiting) in crossings {
                if exiting {
                    self.edges[edge].end = index;
                } else {
                    self.edges[edge].start = index;
                }
            }
            self.edges.push(Edge {
                start,
                end,
                enabled: true,
            });
        } else {
            self.nodes.truncate(initial_nodes);
        }
    }

    /// Native 7D92F0 chooses the first surviving edge, then walks both ends.
    pub fn strip(&self) -> Vec<ClipVertex> {
        let Some(edge) = self.edges.iter().find(|edge| edge.enabled) else {
            return Vec::new();
        };
        let mut left = edge.start;
        let mut right = edge.end;
        let mut strip = Vec::with_capacity(self.nodes.len());
        loop {
            strip.push(self.nodes[left].vertex);
            if left == right {
                break;
            }
            left = self.edges[self.nodes[left].previous].start;
            strip.push(self.nodes[right].vertex);
            if left == right {
                break;
            }
            right = self.edges[self.nodes[right].next].end;
        }
        strip
    }
}

/// The signed distance is stored as f32 before classification and interpolation.
fn distance(position: [f32; 3], plane: [f32; 4], side: i16) -> f32 {
    (((f64::from(position[2]) * f64::from(plane[2])
        + f64::from(position[1]) * f64::from(plane[1]))
        + f64::from(position[0]) * f64::from(plane[0])
        + f64::from(plane[3]))
        * f64::from(side)) as f32
}

/// Native callbacks force x87 truncation for unsigned byte/word interpolation.
fn interpolate(a: ClipVertex, b: ClipVertex, fraction: f64, authored_uv: bool) -> ClipVertex {
    let attribute_fraction = f64::from(fraction as f32);
    let attributes = if authored_uv {
        std::array::from_fn(|axis| {
            (f64::from(a.attributes[axis])
                + (f64::from(b.attributes[axis]) - f64::from(a.attributes[axis]))
                    * attribute_fraction) as u16
        })
    } else {
        [
            (f64::from(a.attributes[0] as u8)
                + (f64::from(b.attributes[0] as u8) - f64::from(a.attributes[0] as u8))
                    * attribute_fraction) as u16,
            0,
        ]
    };
    ClipVertex {
        position: std::array::from_fn(|axis| {
            (f64::from(a.position[axis])
                + (f64::from(b.position[axis]) - f64::from(a.position[axis])) * fraction)
                as f32
        }),
        attributes,
    }
}
