//! Character skill-line rows, header expansion, and pending rank points.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// One row in the authoritative skill-line sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiSkillLine {
    /// A collapsible category header.
    Header {
        /// Localized category label.
        name: String,
        /// Whether the category's following skill rows are visible.
        expanded: bool,
    },
    /// One learned, learnable, or trainable skill.
    Skill(UiSkillLineSkill),
}

impl UiSkillLine {
    /// Creates one expanded localized category header.
    #[must_use]
    pub fn header(name: impl Into<String>) -> Self {
        Self::Header {
            name: name.into(),
            expanded: true,
        }
    }
}

/// Script-visible values for one non-header skill row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSkillLineSkill {
    /// Stable SkillLine DBC identity used to retain selection across projection changes.
    pub skill_id: u32,
    /// Localized skill-line label.
    pub name: String,
    /// Permanent learned rank.
    pub rank: u32,
    /// Temporary rank contribution returned separately by the stock API.
    pub temporary_points: i32,
    /// Effective rank modifier from equipment or auras.
    pub modifier: i32,
    /// Current rank ceiling.
    pub maximum_rank: u32,
    /// Whether the client may request that the profession be abandoned.
    pub abandonable: bool,
    /// Optional points needed to advance the current skill step.
    pub step_cost: Option<u32>,
    /// Optional points charged for the next rank.
    pub rank_cost: Option<u32>,
    /// Minimum character level for the next rank.
    pub minimum_level: u32,
    /// Stock cost-category discriminator.
    pub cost_type: u32,
    /// Localized skill description when the row has one.
    pub description: Option<String>,
}

#[derive(Debug, Default)]
struct UiSkillLineInner {
    lines: Vec<UiSkillLine>,
    selected_skill_id: Option<u32>,
    adjusted_skill_points: i32,
    talent_points: u32,
    skill_points: u32,
}

/// Shared player skill sequence and client-side selection state.
#[derive(Clone, Debug, Default)]
pub struct UiSkillLineState {
    inner: Rc<RefCell<UiSkillLineInner>>,
}

impl UiSkillLineState {
    /// Creates the pre-character skill image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the full sequence after skill fields or DBC joins change.
    pub fn replace(&self, lines: Vec<UiSkillLine>) {
        let mut inner = self.inner.borrow_mut();
        inner.lines = lines;
        inner.selected_skill_id = None;
    }

    /// Sets the points remaining after pending client-side rank changes.
    pub fn set_adjusted_skill_points(&self, points: i32) {
        self.inner.borrow_mut().adjusted_skill_points = points;
    }

    /// Replaces the authoritative unspent talent and profession point pair.
    pub fn set_character_points(&self, talent_points: u32, skill_points: u32) {
        let mut inner = self.inner.borrow_mut();
        inner.talent_points = talent_points;
        inner.skill_points = skill_points;
    }

    fn visible_lines(&self) -> Vec<UiSkillLine> {
        let inner = self.inner.borrow();
        let mut visible = Vec::new();
        let mut expanded = true;
        for line in &inner.lines {
            match line {
                UiSkillLine::Header {
                    expanded: header_expanded,
                    ..
                } => {
                    expanded = *header_expanded;
                    visible.push(line.clone());
                }
                UiSkillLine::Skill(_) if expanded => visible.push(line.clone()),
                UiSkillLine::Skill(_) => {}
            }
        }
        visible
    }

    fn set_selected(&self, index: usize) {
        let selected = index
            .checked_sub(1)
            .and_then(|index| self.visible_lines().get(index).cloned())
            .and_then(|line| match line {
                UiSkillLine::Skill(skill) => Some(skill.skill_id),
                UiSkillLine::Header { .. } => None,
            });
        self.inner.borrow_mut().selected_skill_id = selected;
    }

    fn selected_index(&self) -> usize {
        let selected = self.inner.borrow().selected_skill_id;
        let Some(selected) = selected else { return 0 };
        self.visible_lines()
            .iter()
            .position(
                |line| matches!(line, UiSkillLine::Skill(skill) if skill.skill_id == selected),
            )
            .map_or(0, |index| index + 1)
    }

    fn set_expanded(&self, index: usize, expanded: bool) {
        let mut inner = self.inner.borrow_mut();
        if index == 0 {
            for line in &mut inner.lines {
                if let UiSkillLine::Header {
                    expanded: current, ..
                } = line
                {
                    *current = expanded;
                }
            }
            return;
        }

        let mut visible_index = 0;
        let mut category_expanded = true;
        for line in &mut inner.lines {
            match line {
                UiSkillLine::Header {
                    expanded: current, ..
                } => {
                    visible_index += 1;
                    category_expanded = *current;
                    if visible_index == index {
                        *current = expanded;
                        return;
                    }
                }
                UiSkillLine::Skill(_) if category_expanded => visible_index += 1,
                UiSkillLine::Skill(_) => {}
            }
        }
    }
}

/// Registers the build-12340 skill-line query and selection family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiSkillLineState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    let select_state = state.clone();
    let selected_state = state.clone();
    let collapse_state = state.clone();
    let expand_state = state.clone();
    let points_state = state.clone();
    let character_points_state = state.clone();
    globals.raw_set(
        "GetNumSkillLines",
        lua.create_function(move |_, ()| Ok(state.visible_lines().len()))?,
    )?;
    globals.raw_set(
        "GetSkillLineInfo",
        lua.create_function(move |lua, index: f64| {
            let index = rounded_index(index);
            let line = index
                .checked_sub(1)
                .and_then(|index| info_state.visible_lines().get(index).cloned());
            skill_line_values(lua, line)
        })?,
    )?;
    globals.raw_set(
        "SetSelectedSkill",
        lua.create_function(move |_, index: f64| {
            select_state.set_selected(rounded_index(index));
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetSelectedSkill",
        lua.create_function(move |_, ()| Ok(selected_state.selected_index()))?,
    )?;
    globals.raw_set(
        "CollapseSkillHeader",
        lua.create_function(move |_, index: f64| {
            collapse_state.set_expanded(rounded_index(index), false);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "ExpandSkillHeader",
        lua.create_function(move |_, index: f64| {
            expand_state.set_expanded(rounded_index(index), true);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetAdjustedSkillPoints",
        lua.create_function(move |_, ()| Ok(points_state.inner.borrow().adjusted_skill_points))?,
    )?;
    globals.raw_set(
        "UnitCharacterPoints",
        lua.create_function(move |_, unit: Value| {
            let Value::String(unit) = unit else {
                return Err(mlua::Error::runtime("Usage: UnitCharacterPoints(\"unit\")"));
            };
            if !unit.to_str()?.eq_ignore_ascii_case("player") {
                return Ok((0_u32, 0_u32));
            }
            let inner = character_points_state.inner.borrow();
            Ok((inner.talent_points, inner.skill_points))
        })?,
    )
}

fn rounded_index(index: f64) -> usize {
    if !index.is_finite() || index < 1.0 || index > usize::MAX as f64 {
        0
    } else {
        index.round() as usize
    }
}

fn skill_line_values(lua: &Lua, line: Option<UiSkillLine>) -> mlua::Result<MultiValue> {
    match line {
        Some(UiSkillLine::Header { name, expanded }) => Ok(MultiValue::from_vec(vec![
            Value::String(lua.create_string(name)?),
            numeric_flag(true),
            numeric_flag(expanded),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Integer(0),
            Value::Integer(0),
        ])),
        Some(UiSkillLine::Skill(skill)) => Ok(MultiValue::from_vec(vec![
            Value::String(lua.create_string(skill.name)?),
            Value::Nil,
            Value::Nil,
            Value::Integer(i64::from(skill.rank)),
            Value::Integer(i64::from(skill.temporary_points)),
            Value::Integer(i64::from(skill.modifier)),
            Value::Integer(i64::from(skill.maximum_rank)),
            numeric_flag(skill.abandonable),
            optional_integer(skill.step_cost),
            optional_integer(skill.rank_cost),
            Value::Integer(i64::from(skill.minimum_level)),
            Value::Integer(i64::from(skill.cost_type)),
            skill
                .description
                .map(|text| lua.create_string(text).map(Value::String))
                .transpose()?
                .unwrap_or(Value::Nil),
        ])),
        None => Ok(MultiValue::from_vec(vec![
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Integer(0),
            Value::Integer(0),
            Value::Nil,
        ])),
    }
}

fn numeric_flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}

fn optional_integer(value: Option<u32>) -> Value {
    value.map_or(Value::Nil, |value| Value::Integer(i64::from(value)))
}
