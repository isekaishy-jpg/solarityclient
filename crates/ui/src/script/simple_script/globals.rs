//! Process-independent globals installed before built-in UI execution.

use mlua::{Function, Lua, LuaString, MultiValue, Table, Value, Variadic};

use solarity_asset::{CharacterClassCatalog, Locale};

use crate::{UiCharacterInfo, UiGlueNetworkAction, UiLoginRequest, UiManifestKind, UiRealmInfo};

use super::UiScriptEnvironment;
use super::cvars::UiCVarSetError;
use super::{portrait_unit_key, texture_file_key, texture_solid_color_key, type_key};

mod legal_agreement;
mod scan_dll;

const ERROR_HANDLER_REGISTRY: &str = "solarity.ui.error_handler";
const CHARACTER_SELECT_MODEL_REGISTRY: &str = "solarity.ui.character_select_model";
const CHARACTER_CUSTOMIZE_MODEL_REGISTRY: &str = "solarity.ui.character_customize_model";

pub(super) fn register_base_globals(
    lua: &Lua,
    environment: &UiScriptEnvironment,
    manifest_kind: UiManifestKind,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let (screen_width, screen_height) = environment.ui_extent();
    globals.raw_set(
        "GetScreenWidth",
        lua.create_function(move |_, ()| Ok(screen_width))?,
    )?;
    globals.raw_set(
        "GetScreenHeight",
        lua.create_function(move |_, ()| Ok(screen_height))?,
    )?;
    globals.raw_set(
        "IsWindowsClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "windows").then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "IsMacClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "macos").then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "IsLinuxClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "linux").then_some(1_u32)))?,
    )?;
    // Manifest-owned FrameXML and GlueXML execute in the stock secure context.
    // Add-on provenance will extend this state when untrusted manifests load.
    globals.raw_set("issecure", lua.create_function(|_, ()| Ok(true))?)?;
    globals.raw_set("securecall", create_secure_call(lua, "securecall")?)?;
    globals.raw_set(
        "securecallfunction",
        create_secure_call(lua, "securecallfunction")?,
    )?;
    register_bit_library(lua, &globals)?;
    register_localized_class_list(lua, &globals, environment)?;
    register_static_constants(lua, &globals)?;
    register_item_quality_color(lua, &globals)?;
    register_client_runtime_globals(lua, &globals, environment)?;
    register_sound_globals(lua, &globals, environment)?;
    register_portrait_globals(lua, &globals)?;
    register_addon_globals(lua, &globals, environment.addon_load_state())?;
    register_saved_variable_globals(lua, &globals, environment.saved_variable_state())?;
    match manifest_kind {
        UiManifestKind::Glue => register_glue_globals(lua, &globals, environment)?,
        UiManifestKind::Frame => register_frame_globals(lua, &globals, environment)?,
    }
    globals.raw_set(
        "seterrorhandler",
        lua.create_function(|lua, handler: Value| {
            let Value::Function(handler) = handler else {
                return Err(mlua::Error::runtime("Usage: seterrorhandler(errfunc)"));
            };
            lua.set_named_registry_value(ERROR_HANDLER_REGISTRY, handler)
        })?,
    )?;
    globals.raw_set(
        "geterrorhandler",
        lua.create_function(|lua, ()| {
            lua.named_registry_value::<Option<Function>>(ERROR_HANDLER_REGISTRY)
        })?,
    )?;
    register_table_wipe(lua)?;
    lua.load(COMPATIBILITY_SOURCE)
        .set_name("compat.lua")
        .exec()?;
    Ok(())
}

fn register_saved_variable_globals(
    lua: &Lua,
    globals: &Table,
    state: crate::UiSavedVariableState,
) -> mlua::Result<()> {
    let character_state = state.clone();
    globals.raw_set(
        "RegisterForSave",
        lua.create_function(move |_, name: Value| {
            let Value::String(name) = name else {
                return Err(mlua::Error::runtime("Usage: RegisterForSave(\"variable\")"));
            };
            state.register_account(name.to_str()?.to_owned());
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "RegisterForSavePerCharacter",
        lua.create_function(move |_, name: Value| {
            let Value::String(name) = name else {
                return Err(mlua::Error::runtime(
                    "Usage: RegisterForSavePerCharacter(\"variable\")",
                ));
            };
            character_state.register_character(name.to_str()?.to_owned());
            Ok(())
        })?,
    )
}

/// Installs native APIs whose backing state exists only in an active world.
fn register_frame_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    crate::feature::register_account_globals(lua, globals, environment.account_state())?;
    crate::feature::register_action_bar_globals(lua, globals, environment.action_bar_state())?;
    crate::feature::register_battlefield_globals(lua, globals, environment.battlefield_state())?;
    crate::feature::register_chat_type_globals(lua, globals)?;
    crate::feature::register_chat_window_globals(lua, globals, environment.chat_window_state())?;
    crate::feature::register_channel_globals(lua, globals, environment.channel_state())?;
    crate::feature::register_companion_globals(lua, globals, environment.companion_state())?;
    crate::feature::register_loot_globals(lua, globals, environment.loot_state())?;
    crate::feature::register_mail_globals(lua, globals, environment.mail_compose_state())?;
    crate::feature::register_minimap_globals(lua, globals, environment.minimap_tracking_state())?;
    crate::feature::register_paper_doll_globals(lua, globals, environment.assets())?;
    crate::feature::register_pet_action_globals(lua, globals, environment.pet_action_state())?;
    crate::feature::register_group_finder_globals(lua, globals, environment.group_finder_state())?;
    crate::feature::register_group_roster_globals(lua, globals, environment.group_roster_state())?;
    crate::feature::register_guild_globals(lua, globals, environment.guild_state())?;
    crate::feature::register_quest_log_globals(lua, globals, environment.quest_log_state())?;
    crate::feature::register_rune_globals(lua, globals, environment.rune_state())?;
    crate::feature::register_skill_globals(lua, globals, environment.skill_line_state())?;
    crate::feature::register_social_globals(lua, globals, environment.social_query_state())?;
    crate::feature::register_spell_book_globals(lua, globals, environment.spell_book_state())?;
    crate::feature::register_stance_globals(lua, globals, environment.stance_state())?;
    crate::feature::register_tabard_globals(lua, globals, environment.tabard_state())?;
    crate::feature::register_voice_chat_globals(lua, globals, environment.voice_chat_state())?;
    crate::feature::register_world_map_globals(lua, globals, environment.world_map_state())?;
    crate::feature::register_world_state_ui_globals(
        lua,
        globals,
        environment.world_state_ui_state(),
    )?;
    register_modifier_globals(lua, globals, environment.modifier_key_state())?;
    let world = environment.world_state();
    let unit_xp = world.clone();
    let unit_xp_max = world.clone();
    let unit_faction = world.clone();
    let default_language = world.clone();
    let zone_text = world.clone();
    let real_zone_text = world.clone();
    let sub_zone_text = world.clone();
    let minimap_zone_text = world.clone();
    let zone_pvp = world.clone();
    let realm_date = world.clone();
    let realm_time = world.clone();
    let cursor_state = world.clone();
    let trade_state = world.clone();
    let target_trade_state = world.clone();
    let area_resurrection = world.clone();
    let friend_counts = world.clone();
    let threat_warnings = environment.cvars();
    let resting = world.clone();
    globals.raw_set(
        "IsResting",
        lua.create_function(move |_, ()| Ok(resting.is_resting().then_some(1_u8)))?,
    )?;
    globals.raw_set(
        "PartialPlayTime",
        lua.create_function(|_, ()| Ok(Option::<u8>::None))?,
    )?;
    globals.raw_set(
        "NoPlayTime",
        lua.create_function(|_, ()| Ok(Option::<u8>::None))?,
    )?;
    globals.raw_set(
        "IsThreatWarningEnabled",
        lua.create_function(move |_, _unit: Option<String>| {
            let enabled = threat_warnings
                .get("threatWarning")
                .and_then(|value| value.parse::<u8>().ok())
                .is_some_and(|value| value != 0);
            Ok(enabled.then_some(1_u8))
        })?,
    )?;
    let unit_names = world.clone();
    let instance_state = world.clone();
    globals.raw_set(
        "IsInInstance",
        lua.create_function(move |_, ()| {
            let instance_type = instance_state.instance_type();
            Ok((
                (instance_type != crate::UiInstanceType::None).then_some(Value::Number(1.0)),
                instance_type.as_str(),
            ))
        })?,
    )?;
    globals.raw_set(
        "UnitName",
        lua.create_function(move |lua, unit: Value| {
            let Value::String(unit) = unit else {
                return Err(mlua::Error::runtime("Usage: UnitName(\"unit\")"));
            };
            let identity = unit
                .to_str()?
                .eq_ignore_ascii_case("player")
                .then(|| unit_names.player_identity())
                .flatten();
            let mut values = MultiValue::new();
            values.push_back(match identity {
                Some(identity) => Value::String(lua.create_string(identity.name())?),
                None => Value::Nil,
            });
            // Build 12340 returns no realm suffix for the local player.
            values.push_back(Value::Nil);
            Ok(values)
        })?,
    )?;
    register_unit_relation_globals(lua, globals, world.clone())?;
    globals.raw_set(
        "GetNumFriends",
        lua.create_function(move |_, ()| {
            let counts = friend_counts.friend_counts();
            Ok((counts.total(), counts.online()))
        })?,
    )?;
    globals.raw_set(
        "GetMoney",
        lua.create_function(move |_, ()| {
            let player = world.player().ok_or_else(|| {
                mlua::Error::runtime("GetMoney requires authoritative active-player state")
            })?;
            // PUC Lua 5.1 represents numbers as doubles. Every u32 copper
            // value is exactly representable, so this conversion loses no bits.
            Ok(f64::from(player.money_copper()))
        })?,
    )?;
    globals.raw_set(
        "GetCursorMoney",
        lua.create_function(move |_, ()| Ok(f64::from(cursor_state.cursor_money_copper())))?,
    )?;
    globals.raw_set(
        "UnitXP",
        lua.create_function(move |_, unit: String| {
            if unit != "player" {
                return Ok(0.0);
            }
            let progression = unit_xp.player_progression().ok_or_else(|| {
                mlua::Error::runtime("UnitXP requires authoritative local-player progression")
            })?;
            Ok(f64::from(progression.experience()))
        })?,
    )?;
    globals.raw_set(
        "UnitXPMax",
        lua.create_function(move |_, unit: String| {
            if unit != "player" {
                return Ok(0.0);
            }
            let progression = unit_xp_max.player_progression().ok_or_else(|| {
                mlua::Error::runtime("UnitXPMax requires authoritative local-player progression")
            })?;
            Ok(f64::from(progression.next_level_experience()))
        })?,
    )?;
    globals.raw_set(
        "UnitFactionGroup",
        lua.create_function(move |lua, unit: String| {
            if unit != "player" {
                return Ok(MultiValue::new());
            }
            let faction = unit_faction.player_faction().ok_or_else(|| {
                mlua::Error::runtime("UnitFactionGroup requires authoritative player faction")
            })?;
            let mut values = MultiValue::new();
            values.push_back(Value::String(lua.create_string(faction.group().as_str())?));
            values.push_back(Value::String(lua.create_string(faction.name())?));
            Ok(values)
        })?,
    )?;
    globals.raw_set(
        "GetDefaultLanguage",
        lua.create_function(move |lua, ()| {
            let mut values = MultiValue::new();
            if let Some(language) = default_language.player_default_language() {
                values.push_back(Value::String(lua.create_string(language.name())?));
            }
            Ok(values)
        })?,
    )?;
    globals.raw_set(
        "GetPlayerTradeMoney",
        lua.create_function(move |_, ()| Ok(f64::from(trade_state.player_trade_money_copper())))?,
    )?;
    globals.raw_set(
        "GetTargetTradeMoney",
        lua.create_function(move |_, ()| {
            Ok(f64::from(target_trade_state.target_trade_money_copper()))
        })?,
    )?;
    globals.raw_set(
        "CanHearthAndResurrectFromArea",
        lua.create_function(move |_, ()| Ok(area_resurrection.area_resurrection_available()))?,
    )?;
    globals.raw_set(
        "GetZoneText",
        lua.create_function(move |_, ()| {
            zone_text
                .zone()
                .map(|zone| zone.zone_text().to_owned())
                .ok_or_else(|| {
                    mlua::Error::runtime("GetZoneText requires authoritative zone state")
                })
        })?,
    )?;
    globals.raw_set(
        "GetSubZoneText",
        lua.create_function(move |_, ()| {
            sub_zone_text
                .zone()
                .map(|zone| zone.sub_zone_text().to_owned())
                .ok_or_else(|| {
                    mlua::Error::runtime("GetSubZoneText requires authoritative zone state")
                })
        })?,
    )?;
    globals.raw_set(
        "GetRealZoneText",
        lua.create_function(move |_, ()| {
            real_zone_text
                .zone()
                .map(|zone| zone.real_zone_text().to_owned())
                .ok_or_else(|| {
                    mlua::Error::runtime("GetRealZoneText requires authoritative zone state")
                })
        })?,
    )?;
    globals.raw_set(
        "GetMinimapZoneText",
        lua.create_function(move |_, ()| {
            minimap_zone_text
                .zone()
                .map(|zone| zone.minimap_zone_text().to_owned())
                .ok_or_else(|| {
                    mlua::Error::runtime("GetMinimapZoneText requires authoritative zone state")
                })
        })?,
    )?;
    globals.raw_set(
        "GetZonePVPInfo",
        lua.create_function(move |_, ()| {
            let zone = zone_pvp.zone().ok_or_else(|| {
                mlua::Error::runtime("GetZonePVPInfo requires authoritative zone state")
            })?;
            Ok((
                zone.pvp_type().map(crate::UiZonePvpType::as_str),
                zone.is_sub_zone_pvp().then_some(1_u32),
                zone.faction_name().map(str::to_owned),
            ))
        })?,
    )?;
    globals.raw_set(
        "CalendarGetDate",
        lua.create_function(move |_, ()| {
            realm_date
                .realm_date()
                .map(|date| (date.weekday(), date.month(), date.month_day(), date.year()))
                .ok_or_else(|| {
                    mlua::Error::runtime("CalendarGetDate requires authoritative realm date")
                })
        })?,
    )?;
    globals.raw_set(
        "GetGameTime",
        lua.create_function(move |_, ()| {
            realm_time
                .realm_time()
                .map(|time| (time.hour(), time.minute()))
                .ok_or_else(|| {
                    mlua::Error::runtime("GetGameTime requires authoritative realm time")
                })
        })?,
    )
}

fn register_unit_relation_globals(
    lua: &Lua,
    globals: &Table,
    world: crate::UiWorldState,
) -> mlua::Result<()> {
    let classes = world.clone();
    globals.raw_set(
        "UnitClass",
        lua.create_function(move |lua, unit: String| {
            let class = unit
                .eq_ignore_ascii_case("player")
                .then(|| classes.player_class())
                .flatten();
            let mut values = MultiValue::new();
            if let Some(class) = class {
                values.push_back(Value::String(lua.create_string(class.name())?));
                values.push_back(Value::String(lua.create_string(class.token())?));
                values.push_back(Value::Integer(i64::from(class.id())));
            } else {
                values.extend([Value::Nil, Value::Nil, Value::Nil]);
            }
            Ok(values)
        })?,
    )?;
    let races = world.clone();
    globals.raw_set(
        "UnitRace",
        lua.create_function(move |lua, unit: String| {
            let race = unit
                .eq_ignore_ascii_case("player")
                .then(|| races.player_race())
                .flatten();
            let mut values = MultiValue::new();
            if let Some(race) = race {
                values.push_back(Value::String(lua.create_string(race.name())?));
                values.push_back(Value::String(lua.create_string(race.token())?));
                values.push_back(Value::Integer(i64::from(race.id())));
            } else {
                values.extend([Value::Nil, Value::Nil, Value::Nil]);
            }
            Ok(values)
        })?,
    )?;
    let health = world.clone();
    globals.raw_set(
        "UnitHealth",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&health, &unit).map_or(0, crate::UiPlayerVitalsState::health))
        })?,
    )?;
    let max_health = world.clone();
    globals.raw_set(
        "UnitHealthMax",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&max_health, &unit).map_or(0, crate::UiPlayerVitalsState::max_health))
        })?,
    )?;
    for name in ["UnitMana", "UnitPower"] {
        let power = world.clone();
        globals.raw_set(
            name,
            lua.create_function(move |_, unit: String| {
                Ok(unit_vitals(&power, &unit).map_or(0, crate::UiPlayerVitalsState::power))
            })?,
        )?;
    }
    for name in ["UnitManaMax", "UnitPowerMax"] {
        let max_power = world.clone();
        globals.raw_set(
            name,
            lua.create_function(move |_, unit: String| {
                Ok(unit_vitals(&max_power, &unit).map_or(0, crate::UiPlayerVitalsState::max_power))
            })?,
        )?;
    }
    let power_types = world.clone();
    globals.raw_set(
        "UnitPowerType",
        lua.create_function(move |_, unit: String| {
            let power_type = unit_vitals(&power_types, &unit)
                .map(crate::UiPlayerVitalsState::power_type)
                .unwrap_or(crate::UiUnitPowerType::Mana);
            Ok((power_type.id(), power_type.as_str()))
        })?,
    )?;
    let connected = world.clone();
    globals.raw_set(
        "UnitIsConnected",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&connected, &unit)
                .is_some_and(crate::UiPlayerVitalsState::connected)
                .then_some(1_u8))
        })?,
    )?;
    let dead = world.clone();
    globals.raw_set(
        "UnitIsDead",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&dead, &unit)
                .is_some_and(crate::UiPlayerVitalsState::dead)
                .then_some(1_u8))
        })?,
    )?;
    let ghost = world.clone();
    globals.raw_set(
        "UnitIsGhost",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&ghost, &unit)
                .is_some_and(crate::UiPlayerVitalsState::ghost)
                .then_some(1_u8))
        })?,
    )?;
    let dead_or_ghost = world.clone();
    globals.raw_set(
        "UnitIsDeadOrGhost",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&dead_or_ghost, &unit)
                .is_some_and(|vitals| vitals.dead() || vitals.ghost())
                .then_some(1_u8))
        })?,
    )?;
    let threat = world.clone();
    globals.raw_set(
        "UnitThreatSituation",
        lua.create_function(move |_, unit: String| {
            Ok(unit_vitals(&threat, &unit).and_then(crate::UiPlayerVitalsState::threat_situation))
        })?,
    )?;
    let exists = world.clone();
    let levels = world.clone();
    globals.raw_set(
        "UnitLevel",
        lua.create_function(move |_, unit: String| {
            Ok(if unit.eq_ignore_ascii_case("player") {
                levels
                    .player_identity()
                    .map_or(0, |identity| identity.level())
            } else {
                0
            })
        })?,
    )?;
    let dungeon_difficulty = world.clone();
    globals.raw_set(
        "GetDungeonDifficulty",
        lua.create_function(move |_, ()| Ok(dungeon_difficulty.dungeon_difficulty()))?,
    )?;
    let raid_difficulty = world.clone();
    globals.raw_set(
        "GetRaidDifficulty",
        lua.create_function(move |_, ()| Ok(raid_difficulty.raid_difficulty()))?,
    )?;
    let instance_info = world.clone();
    globals.raw_set(
        "GetInstanceInfo",
        lua.create_function(move |_, ()| {
            let instance_type = instance_info.instance_type();
            if instance_type == crate::UiInstanceType::None {
                return Ok((Option::<String>::None, "none", 0_u8, "", 0_u8, 0_u8, false));
            }
            let difficulty = if instance_type == crate::UiInstanceType::Raid {
                instance_info.raid_difficulty()
            } else {
                instance_info.dungeon_difficulty()
            };
            Ok((
                Some(String::new()),
                instance_type.as_str(),
                difficulty,
                "",
                0,
                0,
                false,
            ))
        })?,
    )?;
    globals.raw_set(
        "UnitExists",
        lua.create_function(move |_, unit: Value| {
            let is_player = matches!(unit, Value::String(ref unit) if unit.to_string_lossy().eq_ignore_ascii_case("player"));
            Ok((is_player && exists.player().is_some()).then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "UnitIsUnit",
        lua.create_function(|_, (left, right): (String, String)| {
            Ok(
                (left.eq_ignore_ascii_case("player") && right.eq_ignore_ascii_case("player"))
                    .then_some(1_u8),
            )
        })?,
    )?;
    let cooperative = world.clone();
    globals.raw_set(
        "UnitCanCooperate",
        lua.create_function(move |_, (left, right): (String, String)| {
            Ok((cooperative.player().is_some()
                && left.eq_ignore_ascii_case("player")
                && right.eq_ignore_ascii_case("player"))
            .then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "UnitCanAttack",
        lua.create_function(|_, _: (String, Option<String>)| Ok(Option::<u8>::None))?,
    )?;
    let players = world.clone();
    globals.raw_set(
        "UnitIsPlayer",
        lua.create_function(move |_, unit: String| {
            Ok((players.player().is_some() && unit.eq_ignore_ascii_case("player")).then_some(1_u8))
        })?,
    )?;
    let visible = world.clone();
    globals.raw_set(
        "UnitIsVisible",
        lua.create_function(move |_, unit: Value| {
            let is_player = matches!(unit, Value::String(ref unit) if unit.to_string_lossy().eq_ignore_ascii_case("player"));
            Ok((is_player && visible.player().is_some()).then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "UnitInBattleground",
        lua.create_function(move |_, unit: String| {
            let in_battleground = matches!(
                world.instance_type(),
                crate::UiInstanceType::Pvp | crate::UiInstanceType::Arena
            );
            Ok((unit.eq_ignore_ascii_case("player") && in_battleground).then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "GetSummonFriendCooldown",
        lua.create_function(|_, ()| Ok((0.0_f64, 0.0_f64)))?,
    )?;
    for name in ["IsReferAFriendLinked", "CanSummonFriend", "CanGrantLevel"] {
        globals.raw_set(
            name,
            lua.create_function(|_, _arguments: Variadic<Value>| Ok(Option::<u8>::None))?,
        )?;
    }
    for name in ["UnitIsPVP", "UnitIsPVPFreeForAll", "UnitIsPVPFlagged"] {
        globals.raw_set(
            name,
            lua.create_function(|_, _unit: String| Ok(Option::<u8>::None))?,
        )?;
    }
    for name in [
        "UnitHasVehicleUI",
        "UnitInVehicle",
        "CanExitVehicle",
        "UnitIsPossessed",
    ] {
        globals.raw_set(
            name,
            lua.create_function(|_, _arguments: Variadic<Value>| Ok(Option::<u8>::None))?,
        )?;
    }
    for name in ["UnitBuff", "UnitDebuff", "UnitAura"] {
        globals.raw_set(
            name,
            lua.create_function(|_, _arguments: Variadic<Value>| Ok(MultiValue::new()))?,
        )?;
    }
    Ok(())
}

fn unit_vitals(world: &crate::UiWorldState, unit: &str) -> Option<crate::UiPlayerVitalsState> {
    unit.eq_ignore_ascii_case("player")
        .then(|| world.player_vitals())
        .flatten()
}

/// Registers the exact two-result AddOn progress query used by stock FrameXML.
fn register_addon_globals(
    lua: &Lua,
    globals: &Table,
    addons: crate::UiAddonLoadState,
) -> mlua::Result<()> {
    globals.raw_set(
        "IsAddOnLoaded",
        lua.create_function(move |_, identifier: Value| {
            let status = match identifier {
                Value::Integer(index) => {
                    let index = usize::try_from(index).ok();
                    let Some(index) = index.filter(|index| *index > 0) else {
                        return Err(mlua::Error::runtime(format!(
                            "AddOn index must be in the range of 1 to {}",
                            addons.addon_count()
                        )));
                    };
                    addons.status_by_index(index).ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "AddOn index must be in the range of 1 to {}",
                            addons.addon_count()
                        ))
                    })?
                }
                Value::Number(index) => {
                    let rounded = index.round();
                    let Some(index) =
                        (rounded.is_finite() && rounded >= 1.0).then_some(rounded as usize)
                    else {
                        return Err(mlua::Error::runtime(format!(
                            "AddOn index must be in the range of 1 to {}",
                            addons.addon_count()
                        )));
                    };
                    addons.status_by_index(index).ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "AddOn index must be in the range of 1 to {}",
                            addons.addon_count()
                        ))
                    })?
                }
                Value::String(name) => addons
                    .status_by_name(name.to_str()?.as_ref())
                    .unwrap_or((false, false)),
                _ => {
                    return Err(mlua::Error::runtime(
                        "Usage: IsAddOnLoaded(index or \"name\")",
                    ));
                }
            };
            Ok((
                status.0.then_some(Value::Number(1.0)),
                status.1.then_some(Value::Number(1.0)),
            ))
        })?,
    )
}

fn register_modifier_globals(
    lua: &Lua,
    globals: &Table,
    state: crate::UiModifierKeyState,
) -> mlua::Result<()> {
    let left_shift = state.clone();
    let right_shift = state.clone();
    let shift = state.clone();
    let left_control = state.clone();
    let right_control = state.clone();
    let control = state.clone();
    let left_alt = state.clone();
    let right_alt = state.clone();
    globals.raw_set(
        "IsLeftShiftKeyDown",
        lua.create_function(move |_, ()| Ok(left_shift.keys().left_shift()))?,
    )?;
    globals.raw_set(
        "IsRightShiftKeyDown",
        lua.create_function(move |_, ()| Ok(right_shift.keys().right_shift()))?,
    )?;
    globals.raw_set(
        "IsShiftKeyDown",
        lua.create_function(move |_, ()| Ok(shift.keys().shift()))?,
    )?;
    globals.raw_set(
        "IsLeftControlKeyDown",
        lua.create_function(move |_, ()| Ok(left_control.keys().left_control()))?,
    )?;
    globals.raw_set(
        "IsRightControlKeyDown",
        lua.create_function(move |_, ()| Ok(right_control.keys().right_control()))?,
    )?;
    globals.raw_set(
        "IsControlKeyDown",
        lua.create_function(move |_, ()| Ok(control.keys().control()))?,
    )?;
    globals.raw_set(
        "IsLeftAltKeyDown",
        lua.create_function(move |_, ()| Ok(left_alt.keys().left_alt()))?,
    )?;
    globals.raw_set(
        "IsRightAltKeyDown",
        lua.create_function(move |_, ()| Ok(right_alt.keys().right_alt()))?,
    )?;
    globals.raw_set(
        "IsAltKeyDown",
        lua.create_function(move |_, ()| Ok(state.keys().alt()))?,
    )
}

fn register_item_quality_color(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    const COLORS: [(f64, f64, f64, &str); 9] = [
        (1.00, 1.00, 1.00, "ffffffff"),
        (0.62, 0.62, 0.62, "ff9d9d9d"),
        (1.00, 1.00, 1.00, "ffffffff"),
        (0.12, 1.00, 0.00, "ff1eff00"),
        (0.00, 0.44, 0.87, "ff0070dd"),
        (0.64, 0.21, 0.93, "ffa335ee"),
        (1.00, 0.50, 0.00, "ffff8000"),
        (0.90, 0.80, 0.50, "ffe6cc80"),
        (0.00, 0.80, 1.00, "ff00ccff"),
    ];
    globals.raw_set(
        "GetItemQualityColor",
        lua.create_function(|_, quality: i32| {
            let index = quality
                .checked_add(1)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| mlua::Error::runtime("invalid item quality"))?;
            COLORS
                .get(index)
                .copied()
                .ok_or_else(|| mlua::Error::runtime("invalid item quality"))
        })?,
    )
}

fn register_static_constants(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.raw_set(
        "RegisterStaticConstants",
        lua.create_function(|_, output: Table| {
            // These are the six build-12340 LagReportType wire values. The
            // stock HelpFrame passes the resulting integer to GMReportLag.
            for (name, value) in [
                ("Loot", 1_u8),
                ("AuctionHouse", 2),
                ("Mail", 3),
                ("Chat", 4),
                ("Movement", 5),
                ("Spell", 6),
            ] {
                output.raw_set(name, value)?;
            }
            Ok(())
        })?,
    )
}

fn register_bit_library(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    let bit = lua.create_table()?;
    bit.raw_set(
        "tobit",
        lua.create_function(|lua, value: Value| Ok(bit_result(bit_argument(lua, &value)?)))?,
    )?;
    bit.raw_set(
        "bnot",
        lua.create_function(|lua, value: Value| Ok(bit_result(!bit_argument(lua, &value)?)))?,
    )?;
    bit.raw_set(
        "band",
        lua.create_function(|lua, values: Variadic<Value>| {
            let mut result = u32::MAX;
            for value in values {
                result &= bit_argument(lua, &value)?;
            }
            Ok(bit_result(result))
        })?,
    )?;
    bit.raw_set(
        "bor",
        lua.create_function(|lua, values: Variadic<Value>| {
            let mut result = 0_u32;
            for value in values {
                result |= bit_argument(lua, &value)?;
            }
            Ok(bit_result(result))
        })?,
    )?;
    bit.raw_set(
        "bxor",
        lua.create_function(|lua, values: Variadic<Value>| {
            let mut result = 0_u32;
            for value in values {
                result ^= bit_argument(lua, &value)?;
            }
            Ok(bit_result(result))
        })?,
    )?;
    bit.raw_set(
        "lshift",
        lua.create_function(|lua, (value, shift): (Value, Value)| {
            Ok(bit_result(
                bit_argument(lua, &value)? << (bit_argument(lua, &shift)? & 31),
            ))
        })?,
    )?;
    bit.raw_set(
        "rshift",
        lua.create_function(|lua, (value, shift): (Value, Value)| {
            Ok(bit_result(
                bit_argument(lua, &value)? >> (bit_argument(lua, &shift)? & 31),
            ))
        })?,
    )?;
    bit.raw_set(
        "arshift",
        lua.create_function(|lua, (value, shift): (Value, Value)| {
            let value = bit_argument(lua, &value)? as i32;
            Ok(bit_result(
                (value >> (bit_argument(lua, &shift)? & 31)) as u32,
            ))
        })?,
    )?;
    bit.raw_set(
        "rol",
        lua.create_function(|lua, (value, shift): (Value, Value)| {
            Ok(bit_result(
                bit_argument(lua, &value)?.rotate_left(bit_argument(lua, &shift)? & 31),
            ))
        })?,
    )?;
    bit.raw_set(
        "ror",
        lua.create_function(|lua, (value, shift): (Value, Value)| {
            Ok(bit_result(
                bit_argument(lua, &value)?.rotate_right(bit_argument(lua, &shift)? & 31),
            ))
        })?,
    )?;
    globals.raw_set("bit", bit)
}

fn bit_argument(lua: &Lua, value: &Value) -> mlua::Result<u32> {
    lua.coerce_number(value.clone())?
        .map(|number| number as i64 as u32)
        .ok_or_else(|| mlua::Error::runtime("bit operation requires a number"))
}

const fn bit_result(value: u32) -> i32 {
    value as i32
}

fn register_localized_class_list(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let assets = environment.assets();
    globals.raw_set(
        "FillLocalizedClassList",
        lua.create_function(move |_, (output, female): (Table, bool)| {
            let assets = assets.as_ref().ok_or_else(|| {
                mlua::Error::runtime(
                    "FillLocalizedClassList requires the mounted client database stack",
                )
            })?;
            let classes = CharacterClassCatalog::load(&mut assets.borrow_mut())
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            for class in classes.classes() {
                // The DBC authors file strings in display casing, while this
                // native API exposes the uppercase class tokens consumed by
                // FrameXML's RAID_CLASS_COLORS and CLASS_ICON_TCOORDS tables.
                let token = class.file_string().to_ascii_uppercase();
                let name = if female {
                    class.female_name()
                } else {
                    class.male_name()
                };
                output.raw_set(token, name)?;
            }
            Ok(())
        })?,
    )
}

fn create_secure_call(lua: &Lua, name: &'static str) -> mlua::Result<Function> {
    lua.create_function(move |lua, (target, arguments): (Value, Variadic<Value>)| {
        let function = match target {
            Value::Function(function) => function,
            Value::String(global_name) => lua
                .globals()
                .raw_get::<Function>(global_name.to_str()?)
                .map_err(|_| mlua::Error::runtime(format!("Usage: {name}(function, ...)")))?,
            _ => {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {name}(function, ...)"
                )));
            }
        };
        function.call::<MultiValue>(arguments)
    })
}

/// Installs configuration and hardware queries shared by GlueXML and FrameXML.
fn register_client_runtime_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let client_clock = environment.client_clock();
    globals.raw_set(
        "GetTime",
        lua.create_function(move |_, ()| Ok(client_clock.seconds()))?,
    )?;
    let locale = environment.locale();
    globals.raw_set(
        "GetLocale",
        lua.create_function(move |_, ()| {
            locale
                .map(Locale::as_str)
                .ok_or_else(|| mlua::Error::runtime("GetLocale requires a mounted client locale"))
        })?,
    )?;
    let existing_locales = environment.existing_locales();
    globals.raw_set(
        "GetExistingLocales",
        lua.create_function(move |lua, ()| {
            let mut values = MultiValue::with_capacity(existing_locales.len());
            for locale in existing_locales.iter() {
                values.push_back(Value::String(lua.create_string(locale.as_str())?));
            }
            Ok(values)
        })?,
    )?;
    let battlenet = environment.battlenet_state();
    let connected = battlenet.clone();
    let enabled_and_connected = battlenet.clone();
    let conversation_capacity = battlenet.clone();
    let friend_counts = battlenet.clone();
    globals.raw_set(
        "BNFeaturesEnabled",
        lua.create_function(move |_, ()| Ok(battlenet.features_enabled().then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "BNConnected",
        lua.create_function(move |_, ()| Ok(connected.connected().then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "BNFeaturesEnabledAndConnected",
        lua.create_function(move |_, ()| Ok(enabled_and_connected.connected().then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "BNGetMaxPlayersInConversation",
        lua.create_function(move |_, ()| Ok(conversation_capacity.max_conversation_players()))?,
    )?;
    globals.raw_set(
        "BNGetNumFriends",
        lua.create_function(move |_, ()| Ok(friend_counts.friend_counts()))?,
    )?;
    let bindings = environment.binding_assignments();
    let binding_keys = bindings.clone();
    globals.raw_set(
        "GetBindingKey",
        lua.create_function(move |lua, action: String| {
            let bindings = binding_keys.as_ref().ok_or_else(|| {
                mlua::Error::runtime("GetBindingKey requires authoritative binding assignments")
            })?;
            let bindings = bindings.borrow();
            let mut values = MultiValue::new();
            for key in bindings.keys_for_action(&action).take(2) {
                values.push_back(Value::String(lua.create_string(key.as_str())?));
            }
            Ok(values)
        })?,
    )?;
    globals.raw_set(
        "GetModifiedClick",
        lua.create_function(move |_, action: String| {
            let bindings = bindings.as_ref().ok_or_else(|| {
                mlua::Error::runtime("GetModifiedClick requires authoritative binding assignments")
            })?;
            let bindings = bindings.borrow();
            bindings
                .modified_click(&action)
                .map(|assignment| assignment.chord().as_str().to_owned())
                .ok_or_else(|| {
                    mlua::Error::runtime(format!("unknown modified-click action '{action}'"))
                })
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVar",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVar")?;
            cvars
                .get(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("Couldn't find CVar named '{name}'")))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetCVar",
        lua.create_function(move |lua, (name, value): (Value, Value)| {
            let name = cvar_name(lua, name, "SetCVar")?;
            let value = lua
                .coerce_string(value)?
                .map_or_else(|| "0".to_owned(), |value| value.to_string_lossy());
            set_cvar(&cvars, &name, value)
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVarDefault",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVarDefault")?;
            cvars
                .default_value(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("Couldn't find CVar named '{name}'")))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVarMin",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVarMin")?;
            Ok(cvars.minimum(&name))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVarMax",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVarMax")?;
            Ok(cvars.maximum(&name))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVarBool",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVarBool")?;
            Ok(cvars.boolean(&name))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetGamma",
        lua.create_function(move |_, ()| {
            let value = cvars
                .get("gamma")
                .and_then(|value| value.parse::<f64>().ok())
                .ok_or_else(|| mlua::Error::runtime("stock gamma CVar is invalid"))?;
            Ok(value - 1.0)
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetTerrainMip",
        lua.create_function(move |_, ()| {
            let shadow = cvars
                .get("shadowLevel")
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            Ok(1.0 - shadow)
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetTerrainMip",
        lua.create_function(move |lua, value: Value| {
            let mip = lua.coerce_number(value)?.unwrap_or(0.0);
            set_cvar(&cvars, "shadowLevel", (1.0 - mip.round()).to_string())
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetGamma",
        lua.create_function(move |lua, value: Value| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: SetGamma(value)"))?;
            set_cvar(&cvars, "gamma", (value + 1.0).to_string())
        })?,
    )?;
    globals.raw_set(
        "GetBuildInfo",
        lua.create_function(|lua, ()| {
            let globals = lua.globals();
            Ok((
                globals.raw_get::<String>("VERSION")?,
                globals.raw_get::<String>("RELEASE_BUILD")?,
                "3.3.5",
                "12340",
                "Jun 24 2010",
            ))
        })?,
    )?;
    // The stock login warning specifically tests the client's required SSE
    // execution baseline. Every supported 64-bit Solarity target satisfies it.
    globals.raw_set("IsSystemSupported", lua.create_function(|_, ()| Ok(true))?)?;
    let streaming_trial = environment.streaming_trial();
    globals.raw_set(
        "IsStreamingTrial",
        lua.create_function(move |_, ()| Ok(streaming_trial.then_some(Value::Number(1.0))))?,
    )?;
    globals.raw_set(
        "GetClientExpansionLevel",
        lua.create_function(|_, ()| Ok(3_u32))?,
    )?;
    // The environment supplies one concrete logical display mode. Platform
    // enumeration can widen this list without changing stock one-based indices.
    let logical_extent = environment.logical_extent();
    let resolution = format!("{}x{}", logical_extent.0, logical_extent.1);
    globals.raw_set(
        "GetCurrentResolution",
        lua.create_function(|_, ()| Ok(1_u32))?,
    )?;
    globals.raw_set(
        "GetScreenResolutions",
        lua.create_function(move |_, ()| Ok(resolution.clone()))?,
    )?;
    globals.raw_set(
        "GetRefreshRates",
        lua.create_function(|_, _arguments: Variadic<Value>| Ok(60_u32))?,
    )?;
    globals.raw_set(
        "IsPlayerResolutionAvailable",
        lua.create_function(|_, ()| Ok(true))?,
    )?;
    globals.raw_set(
        "GetCurrentMultisampleFormat",
        lua.create_function(|_, ()| Ok(1_u32))?,
    )?;
    globals.raw_set(
        "GetMultisampleFormats",
        lua.create_function(|_, ()| Ok((24_u32, 24_u32, 1_u32)))?,
    )?;
    // The validator has no attached Vulkan adapter. Stock still reports the
    // four fixed effects flags, a one-sample anisotropy limit, and projected
    // texture support when the adapter-specific anisotropy flag is absent.
    globals.raw_set(
        "GetVideoCaps",
        lua.create_function(|_, ()| Ok((false, true, true, true, true, 1_u32, true)))?,
    )?;
    // No stereo-capable display surface is attached to the headless glue
    // validator, so build 12340 reports the feature as unavailable.
    globals.raw_set(
        "IsStereoVideoAvailable",
        lua.create_function(|_, ()| Ok(None::<u32>))?,
    )?;
    // The platform audio service has not attached devices to this headless
    // environment. Stock represents that state as an empty indexed list.
    globals.raw_set(
        "Sound_GameSystem_GetNumOutputDrivers",
        lua.create_function(|_, ()| Ok(0_u32))?,
    )?;
    globals.raw_set(
        "Sound_GameSystem_GetOutputDriverNameByIndex",
        lua.create_function(|_, _index: u32| Ok(None::<String>))?,
    )?;
    globals.raw_set(
        "Sound_ChatSystem_GetNumInputDrivers",
        lua.create_function(|_, ()| Ok(0_u32))?,
    )?;
    globals.raw_set(
        "Sound_ChatSystem_GetInputDriverNameByIndex",
        lua.create_function(|_, _index: u32| Ok(None::<String>))?,
    )?;
    globals.raw_set(
        "Sound_ChatSystem_GetNumOutputDrivers",
        lua.create_function(|_, ()| Ok(0_u32))?,
    )?;
    globals.raw_set(
        "Sound_ChatSystem_GetOutputDriverNameByIndex",
        lua.create_function(|_, _index: u32| Ok(None::<String>))?,
    )
}

fn register_glue_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    crate::feature::register_account_globals(lua, globals, environment.account_state())?;
    let movie_resolution = environment.logical_extent().0;
    globals.raw_set(
        "GetMovieResolution",
        lua.create_function(move |_, ()| Ok(movie_resolution))?,
    )?;
    let cursor_visible = environment.cursor_visible();
    globals.raw_set(
        "HideCursor",
        lua.create_function(move |_, ()| {
            cursor_visible.set(false);
            Ok(())
        })?,
    )?;
    let cursor_visible = environment.cursor_visible();
    globals.raw_set(
        "ShowCursor",
        lua.create_function(move |_, ()| {
            cursor_visible.set(true);
            Ok(())
        })?,
    )?;
    register_glue_media_globals(lua, globals, environment)?;
    register_glue_network_globals(lua, globals, environment)?;
    legal_agreement::register_globals(lua, globals, environment.cvars())?;
    scan_dll::register_globals(lua, globals)?;
    // A fresh build 12340 process has no renderer or sound options waiting
    // for acknowledgement. AccountLogin's initially shown warning frame asks
    // this native service from OnShow and immediately hides itself.
    globals.raw_set(
        "ShowChangedOptionWarnings",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    globals.raw_set(
        "GetChangedOptionWarnings",
        lua.create_function(|_, ()| Ok(MultiValue::new()))?,
    )?;
    globals.raw_set(
        "AcceptChangedOptionWarnings",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    // The executable owns the current scene name; GlueParent.lua mirrors it
    // into CURRENT_GLUE_SCREEN after selecting a declared GlueScreenInfo frame.
    let current_screen = environment.current_screen();
    let setter_state = current_screen.clone();
    globals.raw_set(
        "SetCurrentScreen",
        lua.create_function(move |lua, value: Value| {
            let value = lua
                .coerce_string(value)?
                .map_or_else(String::new, |value| value.to_string_lossy());
            *setter_state.borrow_mut() = value;
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetCurrentScreen",
        lua.create_function(move |_, ()| Ok(current_screen.borrow().clone()))?,
    )?;
    // Build 12340 stores this authenticator preference outside the named CVar
    // registry; the executable contains no corresponding CVar identifier.
    let uses_token = std::rc::Rc::new(std::cell::Cell::new(false));
    let query_uses_token = uses_token.clone();
    globals.raw_set(
        "GetUsesToken",
        lua.create_function(move |_, ()| Ok(query_uses_token.get()))?,
    )?;
    globals.raw_set(
        "SetUsesToken",
        lua.create_function(move |_, value: bool| {
            uses_token.set(value);
            Ok(())
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetSavedAccountName",
        lua.create_function(move |_, ()| {
            cvars
                .get("accountName")
                .ok_or_else(|| mlua::Error::runtime("stock accountName CVar is not registered"))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetSavedAccountName",
        lua.create_function(move |lua, value: Value| {
            let Some(value) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime(
                    "Usage: SetSavedAccountName(\"accountName\")",
                ));
            };
            set_cvar(&cvars, "accountName", value.to_string_lossy())
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetSavedAccountList",
        lua.create_function(move |_, ()| {
            cvars
                .get("accountList")
                .ok_or_else(|| mlua::Error::runtime("stock accountList CVar is not registered"))
        })?,
    )?;
    register_model_frame_selector(
        lua,
        globals,
        "SetCharSelectModelFrame",
        CHARACTER_SELECT_MODEL_REGISTRY,
    )?;
    register_model_frame_selector(
        lua,
        globals,
        "SetCharCustomizeFrame",
        CHARACTER_CUSTOMIZE_MODEL_REGISTRY,
    )
}

fn register_glue_network_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let network = environment.network();
    globals.raw_set(
        "DefaultServerLogin",
        lua.create_function(move |_, (account, password): (LuaString, LuaString)| {
            let account_name = account.to_str()?.to_string();
            let request = UiLoginRequest::new(account_name, password.as_bytes().to_vec());
            network
                .borrow_mut()
                .push(UiGlueNetworkAction::Login(request));
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "CancelLogin",
        lua.create_function(move |_, ()| {
            network.borrow_mut().push(UiGlueNetworkAction::CancelLogin);
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "DisconnectFromServer",
        lua.create_function(move |_, ()| {
            network.borrow_mut().push(UiGlueNetworkAction::Disconnect);
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetServerName",
        lua.create_function(move |_, ()| {
            let network = network.borrow();
            let status = network.status();
            Ok((
                status.server_name().map(str::to_owned),
                status.player_killing_allowed().then_some(1.0_f64),
                status.roleplaying().then_some(1.0_f64),
                status.is_server_down().then_some(1.0_f64),
            ))
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "IsConnectedToServer",
        lua.create_function(move |_, ()| Ok(network.borrow().status().is_connected()))?,
    )?;
    register_realm_list_globals(lua, globals, environment)?;
    register_character_list_globals(lua, globals, environment)?;
    Ok(())
}

/// Registers the synchronous character-selection surface consumed by
/// `CharacterSelect.lua` after `SMSG_CHAR_ENUM` is published.
fn register_character_list_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let network = environment.network();
    globals.raw_set(
        "GetNumCharacters",
        lua.create_function(move |_, ()| Ok(network.borrow().characters().characters().len()))?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetCharIDFromIndex",
        lua.create_function(move |_, index: u32| {
            Ok(network
                .borrow()
                .characters()
                .by_index(index)
                .map(UiCharacterInfo::guid))
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetIndexFromCharID",
        lua.create_function(move |_, guid: u64| Ok(network.borrow().characters().index_of(guid)))?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetCharacterInfo",
        lua.create_function(move |lua, guid: u64| {
            let network = network.borrow();
            character_info_values(lua, network.characters().by_guid(guid))
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetSelectBackgroundModel",
        lua.create_function(move |_, guid: u64| {
            let network = network.borrow();
            Ok(network.characters().by_guid(guid).map(|character| {
                if character.class_id() == 6 {
                    "DEATHKNIGHT".to_owned()
                } else {
                    character.race_file_string().to_ascii_uppercase()
                }
            }))
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "SelectCharacter",
        lua.create_function(move |_, guid: u64| {
            let mut network = network.borrow_mut();
            if network.select_character(guid) {
                network.push(UiGlueNetworkAction::SelectCharacter { guid });
            }
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "EnterWorld",
        lua.create_function(move |_, ()| {
            let mut network = network.borrow_mut();
            if let Some(guid) = network.characters().selected_guid() {
                network.push(UiGlueNetworkAction::EnterWorld { guid });
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

fn character_info_values(
    lua: &Lua,
    character: Option<&UiCharacterInfo>,
) -> mlua::Result<MultiValue> {
    let Some(character) = character else {
        return Ok(MultiValue::from_vec(vec![Value::Nil]));
    };
    Ok(MultiValue::from_vec(vec![
        Value::String(lua.create_string(character.name())?),
        Value::String(lua.create_string(character.race_name())?),
        Value::String(lua.create_string(character.class_name())?),
        Value::Integer(i64::from(character.level())),
        match character.zone_name() {
            Some(zone) => Value::String(lua.create_string(zone)?),
            None => Value::Nil,
        },
        Value::Integer(i64::from(character.sex())),
        Value::Boolean(character.is_ghost()),
        Value::Boolean(character.has_paid_customization()),
        Value::Boolean(character.has_paid_race_change()),
        Value::Boolean(character.has_paid_faction_change()),
        // Build 12340 computes this from realm/account service eligibility;
        // no disable bit exists in SMSG_CHAR_ENUM itself.
        Value::Boolean(false),
    ]))
}

/// Registers the synchronous realm-list surface consumed by `RealmList.lua`.
fn register_realm_list_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let network = environment.network();
    globals.raw_set(
        "RequestRealmList",
        lua.create_function(move |lua, show_progress_dialog: Option<bool>| {
            let show_progress_dialog = show_progress_dialog.unwrap_or(false);
            let status_message = show_progress_dialog
                .then(|| lua.globals().raw_get::<String>("REALM_LIST_IN_PROGRESS"))
                .transpose()?;
            network
                .borrow_mut()
                .push(UiGlueNetworkAction::RequestRealmList {
                    show_progress_dialog,
                    status_message,
                });
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "RealmListUpdateRate",
        lua.create_function(|_, ()| Ok(5.0_f64))?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "CancelRealmListQuery",
        lua.create_function(move |_, ()| {
            network
                .borrow_mut()
                .push(UiGlueNetworkAction::CancelRealmListQuery);
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetNumRealms",
        lua.create_function(move |_, category_index: Option<u32>| {
            let network = network.borrow();
            let realms = network.realms();
            Ok(category_index.map_or_else(
                || {
                    realms
                        .categories()
                        .iter()
                        .map(|category| category.realms().len())
                        .sum()
                },
                |index| {
                    realms
                        .displayed_category(index)
                        .map_or(0, |category| category.realms().len())
                },
            ))
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetRealmInfo",
        lua.create_function(move |lua, arguments: Variadic<Value>| {
            let (category_index, realm_index) = realm_indices(lua, &arguments, "GetRealmInfo")?;
            let network = network.borrow();
            let realms = network.realms();
            let realm = realms.realm(category_index, realm_index).cloned();
            realm_info_values(lua, realm.as_ref(), realms.selected_realm_id())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "ChangeRealm",
        lua.create_function(move |lua, arguments: Variadic<Value>| {
            let (category_index, realm_index) = realm_indices(lua, &arguments, "ChangeRealm")?;
            let mut network = network.borrow_mut();
            if let Some(category_index) = category_index {
                network.realms_mut().select_category(category_index);
            }
            let realm_id = network
                .realms()
                .realm(category_index, realm_index)
                .map(UiRealmInfo::id);
            if let Some(realm_id) = realm_id {
                network.push(UiGlueNetworkAction::ChangeRealm { realm_id });
            }
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetRealmCategories",
        lua.create_function(move |lua, ()| {
            let network = network.borrow();
            let realms = network.realms();
            let mut names = realms
                .displayed_categories()
                .map(|category| category.name().to_owned())
                .collect::<Vec<_>>();
            if names.is_empty() {
                names.extend(
                    realms
                        .categories()
                        .first()
                        .map(|category| category.name().to_owned()),
                );
            }
            names
                .into_iter()
                .map(|name| lua.create_string(name).map(Value::String))
                .collect::<mlua::Result<Vec<_>>>()
                .map(MultiValue::from_vec)
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "SetPreferredInfo",
        lua.create_function(move |lua, arguments: Variadic<Value>| {
            let category_index = required_one_based_index(
                lua,
                arguments.first(),
                "Usage: SetPreferredInfo(index, pvp, rp)",
            )?;
            let player_killing_allowed = arguments.get(1).is_some_and(lua_truthy);
            let roleplaying = arguments.get(2).is_some_and(lua_truthy);
            network
                .borrow_mut()
                .push(UiGlueNetworkAction::SetPreferredRealmInfo {
                    category_index,
                    player_killing_allowed,
                    roleplaying,
                });
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "SortRealms",
        lua.create_function(move |_, _arguments: Variadic<Value>| {
            network.borrow_mut().push(UiGlueNetworkAction::SortRealms);
            Ok(())
        })?,
    )?;
    let network = environment.network();
    globals.raw_set(
        "GetSelectedCategory",
        lua.create_function(move |_, ()| {
            Ok(network.borrow().realms().selected_displayed_category())
        })?,
    )?;
    let network = environment.network();
    let current_screen = environment.current_screen();
    globals.raw_set(
        "RealmListDialogCancelled",
        lua.create_function(move |_, ()| {
            let from_login_screen = current_screen.borrow().eq_ignore_ascii_case("login");
            network
                .borrow_mut()
                .push(UiGlueNetworkAction::RealmListDialogCancelled { from_login_screen });
            Ok(())
        })?,
    )?;
    register_category_predicate(
        lua,
        globals,
        environment,
        "IsInvalidTournamentRealmCategory",
        |category| category.is_invalid_tournament(),
    )?;
    register_category_predicate(
        lua,
        globals,
        environment,
        "IsTournamentRealmCategory",
        |category| category.is_tournament(),
    )?;
    register_category_predicate(lua, globals, environment, "IsInvalidLocale", |category| {
        category.is_invalid_locale()
    })
}

/// Registers one category predicate while preserving stock one-based indices.
fn register_category_predicate(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
    name: &'static str,
    predicate: fn(&crate::UiRealmCategory) -> bool,
) -> mlua::Result<()> {
    let network = environment.network();
    globals.raw_set(
        name,
        lua.create_function(move |_, category_index: u32| {
            Ok(network
                .borrow()
                .realms()
                .displayed_category(category_index)
                .is_some_and(predicate))
        })?,
    )
}

/// Parses stock's one-argument flat index or two-argument category/index form.
fn realm_indices(
    lua: &Lua,
    arguments: &[Value],
    function: &'static str,
) -> mlua::Result<(Option<u32>, u32)> {
    let usage = match function {
        "GetRealmInfo" => "Usage: GetRealmInfo(category, index)",
        _ => "Usage: ChangeRealm(category, index)",
    };
    let first = required_one_based_index(lua, arguments.first(), usage)?;
    let second = arguments
        .get(1)
        .and_then(|value| lua.coerce_number(value.clone()).ok().flatten())
        .and_then(number_to_one_based_index);
    Ok(second.map_or((None, first), |second| (Some(first), second)))
}

/// Converts a Lua numeric argument to stock's positive one-based index domain.
fn required_one_based_index(
    lua: &Lua,
    value: Option<&Value>,
    usage: &'static str,
) -> mlua::Result<u32> {
    value
        .and_then(|value| lua.coerce_number(value.clone()).ok().flatten())
        .and_then(number_to_one_based_index)
        .ok_or_else(|| mlua::Error::runtime(usage))
}

/// Matches Lua 5.1 integer truncation while rejecting non-positive indices.
fn number_to_one_based_index(number: f64) -> Option<u32> {
    if !number.is_finite() || number < 1.0 || number > f64::from(u32::MAX) {
        return None;
    }
    Some(number.trunc() as u32)
}

/// Applies Lua's native Boolean conversion used by stock `StringToBOOL`.
fn lua_truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

/// Produces all fourteen return values of stock `GetRealmInfo`.
fn realm_info_values(
    lua: &Lua,
    realm: Option<&UiRealmInfo>,
    selected_realm_id: Option<u32>,
) -> mlua::Result<MultiValue> {
    let Some(realm) = realm else {
        return Ok(MultiValue::from_vec(vec![
            Value::Nil,
            Value::Number(0.0),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Number(0.0),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ]));
    };

    let flags = realm.flags();
    let mut values = vec![
        Value::String(lua.create_string(realm.name())?),
        Value::Number(f64::from(realm.character_count())),
        lua_flag(flags.is_invalid()),
        lua_flag(flags.is_offline()),
        lua_flag(selected_realm_id == Some(realm.id())),
        lua_flag(flags.player_killing_allowed()),
        lua_flag(flags.roleplaying()),
        Value::Number(realm.load()),
        lua_flag(flags.is_locked()),
    ];
    if let Some(version) = realm.version() {
        values.extend([
            Value::Number(f64::from(version.major())),
            Value::Number(f64::from(version.minor())),
            Value::Number(f64::from(version.revision())),
            Value::Number(f64::from(version.build())),
            Value::Number(f64::from(version.realm_type())),
        ]);
    } else {
        values.extend([Value::Nil, Value::Nil, Value::Nil, Value::Nil, Value::Nil]);
    }
    Ok(MultiValue::from_vec(values))
}

/// Stock represents a false realm-list flag as nil and true as numeric one.
fn lua_flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}

fn register_glue_media_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    for name in ["PlayMusic", "PlayGlueMusic", "PlayCreditsMusic"] {
        let state = environment.media_intent();
        globals.raw_set(
            name,
            lua.create_function(move |lua, (value, _extra): (Value, Variadic<Value>)| {
                let resource = required_string(lua, value, "music resource")?;
                state.borrow_mut().music = Some(resource);
                Ok(())
            })?,
        )?;
    }
    let state = environment.media_intent();
    globals.raw_set(
        "PlayGlueAmbience",
        lua.create_function(move |lua, (value, _extra): (Value, Variadic<Value>)| {
            let resource = required_string(lua, value, "ambience resource")?;
            state.borrow_mut().ambience = Some(resource);
            Ok(())
        })?,
    )?;
    for name in ["StopMusic", "StopGlueMusic"] {
        let state = environment.media_intent();
        globals.raw_set(
            name,
            lua.create_function(move |_, ()| {
                state.borrow_mut().music = None;
                Ok(())
            })?,
        )?;
    }
    let state = environment.media_intent();
    globals.raw_set(
        "StopGlueAmbience",
        lua.create_function(move |_, ()| {
            state.borrow_mut().ambience = None;
            Ok(())
        })?,
    )?;
    globals.raw_set("StopAllSFX", lua.create_function(|_, ()| Ok(()))?)?;
    Ok(())
}

fn register_sound_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    for name in ["PlaySound", "PlaySoundFile"] {
        let state = environment.media_intent();
        globals.raw_set(
            name,
            lua.create_function(move |lua, (value, _extra): (Value, Variadic<Value>)| {
                let sound = required_string(lua, value, "sound resource")?;
                state.borrow_mut().sounds.push(sound);
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

fn register_portrait_globals(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.raw_set(
        "SetPortraitTexture",
        lua.create_function(|_, (texture, unit): (Table, String)| {
            if texture.raw_get::<String>(type_key())? != "Texture" {
                return Err(mlua::Error::runtime(
                    "Usage: SetPortraitTexture(texture, \"unit\")",
                ));
            }
            texture.raw_set(texture_file_key(), Option::<String>::None)?;
            texture.raw_set(texture_solid_color_key(), Option::<Table>::None)?;
            texture.raw_set(portrait_unit_key(), Some(unit))
        })?,
    )
}

fn required_string(lua: &Lua, value: Value, label: &str) -> mlua::Result<String> {
    lua.coerce_string(value)?
        .map(|value| value.to_string_lossy())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| mlua::Error::runtime(format!("missing {label}")))
}

fn register_model_frame_selector(
    lua: &Lua,
    globals: &Table,
    function: &'static str,
    registry: &'static str,
) -> mlua::Result<()> {
    globals.raw_set(
        function,
        lua.create_function(move |lua, value: Value| {
            let Some(name) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {function}(\"frameName\")"
                )));
            };
            let globals = lua.globals();
            let frame = globals.raw_get::<Option<Table>>(name.to_str()?)?;
            let Some(frame) = frame else {
                return Ok(());
            };
            let kind = frame.raw_get::<String>(type_key())?;
            if matches!(kind.as_str(), "Model" | "ModelFFX") {
                lua.set_named_registry_value(registry, frame)?;
            }
            Ok(())
        })?,
    )
}

fn cvar_name(lua: &Lua, value: Value, function: &str) -> mlua::Result<String> {
    let Some(name) = lua.coerce_string(value)? else {
        return Err(mlua::Error::runtime(format!("Usage: {function}(\"cvar\")")));
    };
    Ok(name.to_string_lossy())
}

fn set_cvar(cvars: &super::cvars::UiCVarRegistry, name: &str, value: String) -> mlua::Result<()> {
    match cvars.set(name, value) {
        Ok(()) => Ok(()),
        Err(UiCVarSetError::Missing) => Err(mlua::Error::runtime(format!(
            "Couldn't find CVar named '{name}'"
        ))),
        Err(UiCVarSetError::ReadOnly) => {
            Err(mlua::Error::runtime(format!("\"{name}\" is read-only")))
        }
    }
}

fn register_table_wipe(lua: &Lua) -> mlua::Result<()> {
    let table: Table = lua.globals().raw_get("table")?;
    table.raw_set(
        "wipe",
        lua.create_function(|_, table: Table| {
            let keys = table
                .pairs::<Value, Value>()
                .map(|entry| entry.map(|(key, _)| key))
                .collect::<mlua::Result<Vec<_>>>()?;
            for key in keys {
                table.raw_set(key, Value::Nil)?;
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}

const COMPATIBILITY_SOURCE: &str = r#"
local tab = table
foreach = tab.foreach
foreachi = tab.foreachi
getn = tab.getn
tinsert = tab.insert
tremove = tab.remove
sort = tab.sort
wipe = tab.wipe

local math = math
abs = math.abs
acos = function (x) return math.deg(math.acos(x)) end
asin = function (x) return math.deg(math.asin(x)) end
atan = function (x) return math.deg(math.atan(x)) end
atan2 = function (x,y) return math.deg(math.atan2(x,y)) end
ceil = math.ceil
cos = function (x) return math.cos(math.rad(x)) end
deg = math.deg
exp = math.exp
floor = math.floor
frexp = math.frexp
ldexp = math.ldexp
log = math.log
log10 = math.log10
max = math.max
min = math.min
mod = math.fmod
PI = math.pi
rad = math.rad
random = math.random
sin = function (x) return math.sin(math.rad(x)) end
sqrt = math.sqrt
tan = function (x) return math.tan(math.rad(x)) end

local str = string
strbyte = str.byte
strchar = str.char
strfind = str.find
format = str.format
gmatch = str.gmatch
gsub = str.gsub
strlen = str.len
strlower = str.lower
strmatch = str.match
strrep = str.rep
strrev = str.reverse
strsub = str.sub
strupper = str.upper
"#;
