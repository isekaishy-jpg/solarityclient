//! CSimpleMessageScrollFrame history ownership (build 12340).

use std::collections::VecDeque;

use crate::UiObjectNode;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MessageConfig {
    pub(crate) maximum: usize,
    pub(crate) display_duration: f64,
    pub(crate) fade_duration: f64,
    pub(crate) fading: bool,
    pub(crate) insert_at_top: bool,
}

impl Default for MessageConfig {
    fn default() -> Self {
        // Factory 0x00812742 passes 8 to 0x0096A2F0. The constructor
        // enables fading, with durations from 0x00AA00FC and 0x00AA0100.
        Self {
            maximum: 8,
            display_duration: 10.0,
            fade_duration: 3.0,
            fading: true,
            insert_at_top: false,
        }
    }
}

impl MessageConfig {
    pub(crate) fn from_node(node: &UiObjectNode<'_>) -> Self {
        let mut config = Self::default();
        for layer in node.layers() {
            for attribute in layer.element().attributes() {
                match attribute.name() {
                    "maxLines" => {
                        if let Ok(value) = attribute.value().parse::<usize>()
                            && value > 0
                        {
                            config.maximum = value;
                        }
                    }
                    "displayDuration" => {
                        if let Ok(value) = attribute.value().parse::<f64>()
                            && value > 0.0
                        {
                            config.display_duration = value;
                        }
                    }
                    "fadeDuration" => {
                        if let Ok(value) = attribute.value().parse::<f64>()
                            && value > 0.0
                        {
                            config.fade_duration = value;
                        }
                    }
                    "fade" => config.fading = matches!(attribute.value(), "true" | "1"),
                    "insertMode" => {
                        config.insert_at_top = attribute.value().eq_ignore_ascii_case("TOP")
                    }
                    _ => {}
                }
            }
        }
        config
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Message {
    pub(crate) text: String,
    pub(crate) color: [f64; 4],
    pub(crate) color_id: i32,
    pub(crate) access_id: i32,
    pub(crate) type_id: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct MessageHistory {
    pub(crate) config: MessageConfig,
    pub(crate) entries: VecDeque<Message>,
    pub(crate) current_line: i32,
    pub(crate) hyperlinks_enabled: bool,
}

impl MessageHistory {
    pub(crate) fn new(config: MessageConfig) -> Self {
        Self {
            config,
            entries: VecDeque::new(),
            current_line: -1,
            hyperlinks_enabled: true,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.current_line = -1;
    }

    pub(crate) fn set_maximum(&mut self, maximum: usize) {
        // 0x009698E0 rebuilds the ring even when the requested size is unchanged.
        self.clear();
        self.config.maximum = maximum;
    }

    pub(crate) fn append(&mut self, message: Message, at_top: bool) {
        if self.entries.len() == self.config.maximum {
            self.entries.pop_front();
        }
        if at_top {
            self.current_line = self.current_line.max(0);
            self.entries.push_front(message);
        } else {
            self.current_line =
                ((i64::from(self.current_line) + 1) % self.config.maximum as i64) as i32;
            self.entries.push_back(message);
        }
    }

    pub(crate) fn count(&self, access_id: i32) -> usize {
        self.entries
            .iter()
            .filter(|entry| access_id == 0 || entry.access_id == access_id)
            .count()
    }

    pub(crate) fn get(&self, index: usize, access_id: i32) -> Option<&Message> {
        self.entries
            .iter()
            .filter(|entry| access_id == 0 || entry.access_id == access_id)
            .nth(index)
    }

    pub(crate) fn presentation_text(&self) -> String {
        let mut text = String::new();
        for index in 0..self.entries.len() {
            let index = if self.config.insert_at_top {
                self.entries.len() - index - 1
            } else {
                index
            };
            let entry = &self.entries[index];
            if !text.is_empty() {
                text.push('\n');
            }
            let [r, g, b, a] = entry
                .color
                .map(|value| (value.clamp(0.0, 1.0) * 255.0) as u8);
            let color = format!("|c{a:02x}{r:02x}{g:02x}{b:02x}");
            text.push_str(&color);
            // Each native message has its own base color. Restore that base
            // after authored |r controls without touching escaped || sequences.
            let mut characters = entry.text.chars().peekable();
            while let Some(character) = characters.next() {
                text.push(character);
                if character == '|'
                    && let Some(next) = characters.next()
                {
                    text.push(next);
                    if next == 'r' {
                        text.push_str(&color);
                    }
                }
            }
            text.push_str("|r");
        }
        text
    }
}
