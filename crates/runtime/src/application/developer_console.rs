//! Tilde-toggle runtime console backed by `egui_console`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, TextureId, Vec2};
use egui_console::{ConsoleBuilder, ConsoleEvent, ConsoleWindow};
use solarity_rendering::{
    UiGlyphTextureHandle, UiMeshPlan, UiPreparedDraw, UiRenderSource, UiRenderVertex,
    VulkanRenderer,
};
use solarity_ui::UiRenderError;

use super::ApplicationError;
use super::ui_frame::PreparedUiFrame;
use crate::input::stock_keyboard_name;
use crate::platform::{
    ButtonState, MouseButton, MouseWheelDirection, PlatformEvent, WindowEvent, WindowId,
};

const CONSOLE_HEIGHT_FRACTION: f32 = 0.48;

struct ConsoleTexture {
    extent: [usize; 2],
    rgba8: Vec<u8>,
    identity: u64,
    handle: UiGlyphTextureHandle,
}

/// Interactive diagnostic console retained independently of GlueXML/FrameXML.
pub(super) struct RuntimeDeveloperConsole {
    context: egui::Context,
    console: ConsoleWindow,
    visible: bool,
    focus_on_next_frame: bool,
    ignore_toggle_text: bool,
    modifiers: Modifiers,
    focused: bool,
    input_events: Vec<Event>,
    started_at: Instant,
    textures: HashMap<TextureId, ConsoleTexture>,
    frames: Vec<PreparedUiFrame>,
    draws: Vec<UiPreparedDraw>,
    logical_extent: [f32; 2],
    recoverable_error_count: u64,
    unique_errors: HashSet<String>,
}

impl RuntimeDeveloperConsole {
    pub(super) fn new(logical_extent: [f32; 2]) -> Self {
        let mut console = ConsoleBuilder::new()
            .prompt("> ")
            .history_size(128)
            .scrollback_size(2_000)
            .build();
        console
            .command_table_mut()
            .extend(["clear", "errors", "help"].into_iter().map(str::to_owned));
        console.write("Solarity developer console. Press ` to close; type help for commands.");
        Self {
            context: egui::Context::default(),
            console,
            visible: false,
            focus_on_next_frame: false,
            ignore_toggle_text: false,
            modifiers: Modifiers::default(),
            focused: true,
            input_events: Vec::new(),
            started_at: Instant::now(),
            textures: HashMap::new(),
            frames: Vec::new(),
            draws: Vec::new(),
            logical_extent,
            recoverable_error_count: 0,
            unique_errors: HashSet::new(),
        }
    }

    pub(super) const fn is_visible(&self) -> bool {
        self.visible
    }

    /// Intercepts console input before GlueXML or FrameXML sees it.
    pub(super) fn service_event(
        &mut self,
        event: &PlatformEvent,
        window_id: WindowId,
        platform_extent: (u32, u32),
    ) -> bool {
        if let PlatformEvent::Key(key) = event
            && key.window_id == window_id
            && key.scan_code.and_then(stock_keyboard_name) == Some("`")
        {
            if key.state == ButtonState::Pressed && !key.is_repeat {
                self.visible = !self.visible;
                self.focus_on_next_frame = self.visible;
                self.ignore_toggle_text = true;
                self.input_events.clear();
                self.draws.clear();
            }
            return true;
        }
        if !self.visible {
            return false;
        }
        self.modifiers = event_modifiers(event).unwrap_or(self.modifiers);
        match event {
            PlatformEvent::Key(key) if key.window_id == window_id => {
                if let Some(key_name) = key.scan_code.and_then(stock_keyboard_name)
                    && let Some(egui_key) = egui_key(key_name)
                {
                    self.input_events.push(Event::Key {
                        key: egui_key,
                        physical_key: Some(egui_key),
                        pressed: key.state == ButtonState::Pressed,
                        repeat: key.is_repeat,
                        modifiers: self.modifiers,
                    });
                }
            }
            PlatformEvent::TextInput(input) if input.window_id == window_id => {
                if self.ignore_toggle_text && matches!(input.text.as_str(), "`" | "~") {
                    self.ignore_toggle_text = false;
                } else {
                    self.ignore_toggle_text = false;
                    self.input_events.push(Event::Text(input.text.clone()));
                }
            }
            PlatformEvent::TextEditing(composition) if composition.window_id == window_id => {
                self.input_events.push(Event::Ime(egui::ImeEvent::Preedit(
                    composition.text.clone(),
                )));
            }
            PlatformEvent::MouseMotion(pointer) if pointer.window_id == window_id => {
                self.input_events.push(Event::PointerMoved(project_pointer(
                    pointer.x,
                    pointer.y,
                    platform_extent,
                    self.logical_extent,
                )));
            }
            PlatformEvent::MouseButton(pointer) if pointer.window_id == window_id => {
                if let Some(button) = egui_pointer_button(pointer.button) {
                    self.input_events.push(Event::PointerButton {
                        pos: project_pointer(
                            pointer.x,
                            pointer.y,
                            platform_extent,
                            self.logical_extent,
                        ),
                        button,
                        pressed: pointer.state == ButtonState::Pressed,
                        modifiers: self.modifiers,
                    });
                }
            }
            PlatformEvent::MouseWheel(wheel) if wheel.window_id == window_id => {
                let direction = match wheel.direction {
                    MouseWheelDirection::Normal => 1.0,
                    MouseWheelDirection::Flipped => -1.0,
                    MouseWheelDirection::Unknown => 0.0,
                };
                self.input_events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(wheel.x * direction, wheel.y * direction),
                    modifiers: self.modifiers,
                });
            }
            PlatformEvent::Window {
                window_id: event_window,
                event: WindowEvent::FocusGained,
            } if *event_window == window_id => {
                self.focused = true;
                self.input_events.push(Event::WindowFocused(true));
            }
            PlatformEvent::Window {
                window_id: event_window,
                event: WindowEvent::FocusLost,
            } if *event_window == window_id => {
                self.focused = false;
                self.input_events.push(Event::WindowFocused(false));
            }
            _ => {}
        }
        true
    }

    pub(super) fn record_error(&mut self, message: &str) {
        self.recoverable_error_count = self.recoverable_error_count.saturating_add(1);
        if !self.unique_errors.insert(message.to_owned()) {
            return;
        }
        self.console
            .write(&format!("[error {}] {message}", self.unique_errors.len()));
    }

    pub(super) fn prepare_frame(
        &mut self,
        renderer: &mut VulkanRenderer,
        elapsed_seconds: f32,
    ) -> Result<(), ApplicationError> {
        if !self.visible {
            self.draws.clear();
            return Ok(());
        }
        let context = self.context.clone();
        let mut events = std::mem::take(&mut self.input_events);
        if self.focus_on_next_frame {
            let position = Pos2::new(
                24.0,
                self.logical_extent[1] * CONSOLE_HEIGHT_FRACTION - 24.0,
            );
            events.extend([
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: self.modifiers,
                },
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: self.modifiers,
                },
            ]);
            self.focus_on_next_frame = false;
        }
        let raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(self.logical_extent[0], self.logical_extent[1]),
            )),
            time: Some(self.started_at.elapsed().as_secs_f64()),
            predicted_dt: elapsed_seconds.max(1.0 / 1_000.0),
            modifiers: self.modifiers,
            events,
            focused: self.focused,
            ..RawInput::default()
        };
        let console_height = self.logical_extent[1] * CONSOLE_HEIGHT_FRACTION;
        let console = &mut self.console;
        let mut console_event = ConsoleEvent::None;
        let output = context.run(raw_input, |context| {
            console_event = egui::TopBottomPanel::top("solarity_developer_console")
                .exact_height(console_height)
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(238))
                        .inner_margin(8.0),
                )
                .show(context, |ui| console.draw(ui))
                .inner;
        });
        self.handle_command(console_event);
        self.update_textures(renderer, &output.textures_delta.set)?;
        let primitives = context.tessellate(output.shapes, output.pixels_per_point);
        self.update_meshes(renderer, primitives)?;
        for texture_id in output.textures_delta.free {
            self.textures.remove(&texture_id);
        }
        Ok(())
    }

    pub(super) const fn logical_extent(&self) -> [f32; 2] {
        self.logical_extent
    }

    pub(super) fn draws(&self) -> &[UiPreparedDraw] {
        &self.draws
    }

    fn handle_command(&mut self, event: ConsoleEvent) {
        let ConsoleEvent::Command(command) = event else {
            return;
        };
        match command.trim().to_ascii_lowercase().as_str() {
            "clear" => self.console.clear(),
            "errors" => self.console.write(&format!(
                "{} recoverable presentation error(s), {} unique, captured this run",
                self.recoverable_error_count,
                self.unique_errors.len()
            )),
            "help" => self
                .console
                .write("commands: clear, errors, help (press ` to close)"),
            "" => {}
            other => self.console.write(&format!("unknown command: {other}")),
        }
        self.console.prompt();
    }

    fn update_textures(
        &mut self,
        renderer: &mut VulkanRenderer,
        deltas: &[(TextureId, egui::epaint::ImageDelta)],
    ) -> Result<(), ApplicationError> {
        for (texture_id, delta) in deltas {
            let egui::ImageData::Color(image) = &delta.image;
            let patch = image
                .pixels
                .iter()
                .flat_map(|color| color.to_srgba_unmultiplied())
                .collect::<Vec<_>>();
            let (extent, rgba8) = if let Some([left, top]) = delta.pos {
                let texture = self.textures.get(texture_id).ok_or_else(|| {
                    ApplicationError::NetworkRuntime {
                        message: "egui supplied a partial console texture before its base image"
                            .to_owned(),
                    }
                })?;
                let mut rgba8 = texture.rgba8.clone();
                for row in 0..image.size[1] {
                    let source = row * image.size[0] * 4;
                    let destination = ((top + row) * texture.extent[0] + left) * 4;
                    let byte_count = image.size[0] * 4;
                    rgba8[destination..destination + byte_count]
                        .copy_from_slice(&patch[source..source + byte_count]);
                }
                (texture.extent, rgba8)
            } else {
                (image.size, patch)
            };
            let identity = next_texture_identity();
            let width =
                u32::try_from(extent[0]).map_err(|_source| ApplicationError::NetworkRuntime {
                    message: "egui console texture width exceeds u32".to_owned(),
                })?;
            let height =
                u32::try_from(extent[1]).map_err(|_source| ApplicationError::NetworkRuntime {
                    message: "egui console texture height exceeds u32".to_owned(),
                })?;
            let handle = renderer.upload_ui_glyph_texture(identity, (width, height), &rgba8)?;
            self.textures.insert(
                *texture_id,
                ConsoleTexture {
                    extent,
                    rgba8,
                    identity,
                    handle,
                },
            );
        }
        Ok(())
    }

    fn update_meshes(
        &mut self,
        renderer: &mut VulkanRenderer,
        primitives: Vec<egui::ClippedPrimitive>,
    ) -> Result<(), ApplicationError> {
        let mut plans = Vec::with_capacity(primitives.len());
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                continue;
            };
            let texture = self.textures.get(&mesh.texture_id).ok_or_else(|| {
                ApplicationError::NetworkRuntime {
                    message: format!(
                        "egui console mesh references missing texture {:?}",
                        mesh.texture_id
                    ),
                }
            })?;
            let vertices = mesh
                .vertices
                .into_iter()
                .map(|vertex| {
                    let [red, green, blue, alpha] = vertex.color.to_srgba_unmultiplied();
                    UiRenderVertex::new(
                        [vertex.pos.x, self.logical_extent[1] - vertex.pos.y],
                        [vertex.uv.x, vertex.uv.y],
                        [
                            f32::from(red) / 255.0,
                            f32::from(green) / 255.0,
                            f32::from(blue) / 255.0,
                            f32::from(alpha) / 255.0,
                        ],
                    )
                })
                .collect();
            let clip = Some([
                primitive.clip_rect.min.x.clamp(0.0, self.logical_extent[0]),
                (self.logical_extent[1] - primitive.clip_rect.max.y)
                    .clamp(0.0, self.logical_extent[1]),
                primitive.clip_rect.max.x.clamp(0.0, self.logical_extent[0]),
                (self.logical_extent[1] - primitive.clip_rect.min.y)
                    .clamp(0.0, self.logical_extent[1]),
            ]);
            let plan = UiMeshPlan::prepare_indexed(
                self.logical_extent,
                vertices,
                mesh.indices,
                UiRenderSource::GlyphAtlas(texture.identity),
                clip,
            )
            .map_err(UiRenderError::from)?;
            plans.push((plan, texture.handle, texture.identity));
        }
        let plan_count = plans.len();
        for (index, (plan, texture, identity)) in plans.into_iter().enumerate() {
            if let Some(frame) = self.frames.get_mut(index)
                && frame.try_replace_compatible_mesh(renderer, &plan)?
            {
                continue;
            }
            let frame = PreparedUiFrame::prepare(
                renderer,
                &plan,
                &HashMap::new(),
                Some((identity, texture)),
            )?;
            if index < self.frames.len() {
                self.frames[index] = frame;
            } else {
                self.frames.push(frame);
            }
        }
        self.frames.truncate(plan_count);
        self.draws.clear();
        for frame in &self.frames {
            self.draws.extend_from_slice(frame.draws());
        }
        Ok(())
    }
}

fn next_texture_identity() -> u64 {
    static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(u64::MAX / 2);
    NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed)
}

const fn event_modifiers(event: &PlatformEvent) -> Option<Modifiers> {
    let modifiers = match event {
        PlatformEvent::Key(key) => key.modifiers,
        _ => return None,
    };
    Some(Modifiers {
        alt: modifiers.has_alt(),
        ctrl: modifiers.has_control(),
        shift: modifiers.has_shift(),
        mac_cmd: false,
        command: modifiers.has_control(),
    })
}

fn egui_key(name: &str) -> Option<Key> {
    Some(match name {
        "DOWN" => Key::ArrowDown,
        "LEFT" => Key::ArrowLeft,
        "RIGHT" => Key::ArrowRight,
        "UP" => Key::ArrowUp,
        "ESCAPE" => Key::Escape,
        "TAB" => Key::Tab,
        "BACKSPACE" => Key::Backspace,
        "ENTER" | "NUMPADENTER" => Key::Enter,
        "SPACE" => Key::Space,
        "INSERT" => Key::Insert,
        "DELETE" => Key::Delete,
        "HOME" => Key::Home,
        "END" => Key::End,
        "PAGEUP" => Key::PageUp,
        "PAGEDOWN" => Key::PageDown,
        "A" => Key::A,
        "C" => Key::C,
        "K" => Key::K,
        "R" => Key::R,
        "U" => Key::U,
        "V" => Key::V,
        "W" => Key::W,
        "X" => Key::X,
        "Y" => Key::Y,
        "Z" => Key::Z,
        "F7" => Key::F7,
        _ => return None,
    })
}

const fn egui_pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        MouseButton::AuxiliaryOne => Some(PointerButton::Extra1),
        MouseButton::AuxiliaryTwo => Some(PointerButton::Extra2),
        MouseButton::Unknown => None,
    }
}

fn project_pointer(x: f32, y: f32, platform_extent: (u32, u32), logical_extent: [f32; 2]) -> Pos2 {
    Pos2::new(
        x / platform_extent.0 as f32 * logical_extent[0],
        y / platform_extent.1 as f32 * logical_extent[1],
    )
}
