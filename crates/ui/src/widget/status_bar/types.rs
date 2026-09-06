//! Build 12340's range validity and ordered XML status-bar properties.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StatusBarState {
    pub minimum: f32,
    pub maximum: f32,
    pub value: f32,
    pub range_valid: bool,
    pub value_valid: bool,
    pub vertical: bool,
    pub rotates_texture: bool,
    pub fill_fraction: Option<f64>,
    pub fill_vertical: bool,
}

impl StatusBarState {
    pub fn valid_range(minimum: f32, maximum: f64) -> bool {
        const LIMIT: f64 = 1_000_000_000_000.0;
        (-LIMIT..=LIMIT).contains(&f64::from(minimum))
            && (-LIMIT..=LIMIT).contains(&maximum)
            && maximum - f64::from(minimum) < LIMIT
    }

    pub fn fraction(self) -> Option<f64> {
        (self.range_valid && self.value_valid).then(|| {
            if self.maximum > self.minimum {
                f64::from((self.value - self.minimum) / (self.maximum - self.minimum))
            } else {
                0.0
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StatusBarXmlOperation {
    Texture(String),
    Color([f64; 4]),
    Range(f32, f32),
    Value(f32),
    Orientation(bool),
    RotatesTexture(bool),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StatusBarConfig(pub Vec<StatusBarXmlOperation>);

impl StatusBarConfig {
    pub fn from_node(object: &crate::UiObjectNode<'_>) -> Self {
        use StatusBarXmlOperation as Op;
        let attribute = |element: &crate::XmlElement, name: &str| {
            element
                .attributes()
                .iter()
                .find(|a| a.name() == name)
                .map(|a| a.value().to_owned())
        };
        let mut operations = Vec::new();
        for layer in object.layers() {
            let element = layer.element();
            let draw_layer = attribute(element, "drawLayer").unwrap_or_else(|| "ARTWORK".into());
            for child in element.content() {
                let crate::XmlContent::Element(child) = child else {
                    continue;
                };
                let Some(child) = layer.document().element(*child) else {
                    continue;
                };
                if child.name().eq_ignore_ascii_case("BarTexture") {
                    operations.push(Op::Texture(draw_layer.clone()));
                } else if child.name().eq_ignore_ascii_case("BarColor") {
                    let color = ["r", "g", "b", "a"].map(|name| {
                        attribute(child, name)
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(if name == "a" { 1.0 } else { 0.0 })
                    });
                    operations.push(Op::Color(color));
                }
            }
            if let (Some(minimum), Some(maximum)) = (
                attribute(element, "minValue").and_then(|v| v.parse::<f32>().ok()),
                attribute(element, "maxValue").and_then(|v| v.parse::<f64>().ok()),
            ) {
                if StatusBarState::valid_range(minimum, maximum) {
                    operations.push(Op::Range(minimum, maximum as f32));
                }
                if let Some(value) =
                    attribute(element, "defaultValue").and_then(|v| v.parse::<f32>().ok())
                {
                    operations.push(Op::Value(value));
                }
            }
            if let Some(axis) = attribute(element, "orientation") {
                if axis.eq_ignore_ascii_case("VERTICAL") {
                    operations.push(Op::Orientation(true));
                } else if axis.eq_ignore_ascii_case("HORIZONTAL") {
                    operations.push(Op::Orientation(false));
                }
            }
            if let Some(value) = attribute(element, "rotatesTexture") {
                operations.push(Op::RotatesTexture(
                    value == "1" || value.eq_ignore_ascii_case("true"),
                ));
            }
        }
        Self(operations)
    }
}
