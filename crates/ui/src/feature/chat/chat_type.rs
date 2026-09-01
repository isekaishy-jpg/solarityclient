//! Stock chat-type identifiers exposed to FrameXML.

use mlua::{Lua, Table, Value};

/// Build 12340's fixed chat records, in native lookup order.
///
/// `GetChatTypeIndex` returns the one-based position in this table. The stock
/// client reserves zero for names that are neither fixed records nor active
/// dynamic records.
const CHAT_TYPE_NAMES: [&str; 62] = [
    "SYSTEM",
    "SAY",
    "PARTY",
    "RAID",
    "GUILD",
    "OFFICER",
    "YELL",
    "WHISPER",
    "WHISPER_FOREIGN",
    "WHISPER_INFORM",
    "EMOTE",
    "TEXT_EMOTE",
    "MONSTER_SAY",
    "MONSTER_PARTY",
    "MONSTER_YELL",
    "MONSTER_WHISPER",
    "MONSTER_EMOTE",
    "CHANNEL",
    "CHANNEL_JOIN",
    "CHANNEL_LEAVE",
    "CHANNEL_LIST",
    "CHANNEL_NOTICE",
    "CHANNEL_NOTICE_USER",
    "AFK",
    "DND",
    "IGNORED",
    "SKILL",
    "LOOT",
    "MONEY",
    "OPENING",
    "TRADESKILLS",
    "PET_INFO",
    "COMBAT_MISC_INFO",
    "COMBAT_XP_GAIN",
    "COMBAT_HONOR_GAIN",
    "COMBAT_FACTION_CHANGE",
    "BG_SYSTEM_NEUTRAL",
    "BG_SYSTEM_ALLIANCE",
    "BG_SYSTEM_HORDE",
    "RAID_LEADER",
    "RAID_WARNING",
    "RAID_BOSS_EMOTE",
    "RAID_BOSS_WHISPER",
    "FILTERED",
    "BATTLEGROUND",
    "BATTLEGROUND_LEADER",
    "RESTRICTED",
    "BATTLENET",
    "ACHIEVEMENT",
    "GUILD_ACHIEVEMENT",
    "ARENA_POINTS",
    "PARTY_LEADER",
    "TARGETICONS",
    "BN_WHISPER",
    "BN_WHISPER_INFORM",
    "BN_CONVERSATION",
    "BN_CONVERSATION_NOTICE",
    "BN_CONVERSATION_LIST",
    "BN_INLINE_TOAST_ALERT",
    "BN_INLINE_TOAST_BROADCAST",
    "BN_INLINE_TOAST_BROADCAST_INFORM",
    "BN_INLINE_TOAST_CONVERSATION",
];

pub(crate) fn register_globals(lua: &Lua, globals: &Table) -> mlua::Result<()> {
    globals.raw_set(
        "GetChatTypeIndex",
        lua.create_function(|_, value: Value| {
            let name = match value {
                Value::String(name) => name,
                // Lua 5.1's string predicate accepts numbers. No fixed stock
                // record has a numeric name, so conversion can only miss.
                Value::Integer(_) | Value::Number(_) => return Ok(0_u32),
                _ => {
                    return Err(mlua::Error::runtime("Usage: GetChatTypeIndex(type)"));
                }
            };
            let name = name.as_bytes();
            let index = CHAT_TYPE_NAMES
                .iter()
                .position(|candidate| candidate.as_bytes().eq_ignore_ascii_case(name.as_ref()));
            Ok(index.map_or(0, |index| index as u32 + 1))
        })?,
    )?;
    Ok(())
}
