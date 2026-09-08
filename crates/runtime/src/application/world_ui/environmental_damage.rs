//! Native environmental combat-log, floating-text and unit-combat arguments.

use crate::application::{
    ApplicationError,
    gameplay_coordinator::environmental_damage::RuntimeEnvironmentalDamageSnapshot,
};
use solarity_ui::{
    FrameManager, UiCombatLogEntry, UiCombatLogObject, UiEventArgument as Arg, UiEventPayload,
};

pub(super) fn dispatch_unit_death(
    manager: &mut FrameManager,
    world: &solarity_ui::UiWorldState,
    death: crate::application::gameplay_coordinator::unit_death::RuntimeUnitDeathSnapshot,
) -> Result<(), ApplicationError> {
    if let Some((health, timer)) = death.player_ui {
        health.publish(world);
        world.set_release_timer(timer);
    }
    let entry = UiCombatLogEntry::new(
        death.clock.timestamp(death.timestamp_ms),
        death.event,
        UiCombatLogObject {
            guid: 0,
            name: None,
            flags: 0x80000000,
        },
        UiCombatLogObject {
            guid: death.identity.guid(),
            name: death.name,
            flags: death.flags,
        },
        None,
        UiEventPayload::empty(),
    )
    .map_err(|error| {
        solarity_ui::UiEventError::from(solarity_ui::UiScriptError::Execution {
            label: death.event.into(),
            message: error.to_string(),
        })
    })?;
    manager.append_combat_log(entry)?;
    Ok(())
}

pub(super) fn dispatch_environmental_damage(
    manager: &mut FrameManager,
    impact: RuntimeEnvironmentalDamageSnapshot,
) -> Result<(), ApplicationError> {
    if !impact.has_combat_event() {
        return Ok(());
    }
    let packet = impact.packet;
    let optional = |value| {
        if value == 0 {
            Arg::Nil
        } else {
            Arg::Integer(i64::from(value))
        }
    };
    let entry = UiCombatLogEntry::new(
        impact.clock.timestamp(impact.timestamp_ms),
        "ENVIRONMENTAL_DAMAGE",
        UiCombatLogObject {
            guid: 0,
            name: None,
            flags: 0x80000000,
        },
        UiCombatLogObject {
            guid: packet.guid,
            name: impact.name,
            flags: impact.flags,
        },
        None,
        UiEventPayload::new(vec![
            Arg::String(impact.kind.token().into()),
            Arg::Number(f64::from(packet.amount)),
            Arg::Number(0.0),
            Arg::Integer(i64::from(impact.kind.school())),
            optional(packet.resisted),
            Arg::Nil,
            optional(packet.absorbed),
            Arg::Nil,
            Arg::Nil,
            Arg::Nil,
        ]),
    )
    .map_err(|error| {
        solarity_ui::UiEventError::from(solarity_ui::UiScriptError::Execution {
            label: "ENVIRONMENTAL_DAMAGE".into(),
            message: error.to_string(),
        })
    })?;
    manager.append_combat_log(entry)?;
    // CombatTextSetActiveUnit can be changed by the combat-log callback above.
    if manager.active_combat_text_unit() == Some(packet.guid) {
        let mut arguments = vec![
            Arg::String(
                if packet.resisted != 0 {
                    "RESIST"
                } else if packet.absorbed != 0 {
                    "ABSORB"
                } else {
                    "DAMAGE"
                }
                .into(),
            ),
            Arg::Integer(i64::from(packet.amount)),
        ];
        if packet.resisted != 0 {
            arguments.push(Arg::Integer(i64::from(packet.resisted)));
        } else if packet.absorbed != 0 {
            arguments.push(Arg::Integer(i64::from(packet.absorbed)));
        }
        manager.dispatch_event("COMBAT_TEXT_UPDATE", &UiEventPayload::new(arguments))?;
    }
    if impact.local_player {
        let label = if packet.amount >= 1 {
            ""
        } else if packet.absorbed != 0 {
            "ABSORB"
        } else if packet.resisted != 0 {
            "RESIST"
        } else {
            ""
        };
        manager.dispatch_event(
            "UNIT_COMBAT",
            &UiEventPayload::new(vec![
                Arg::String("player".into()),
                Arg::String("WOUND".into()),
                Arg::String(label.into()),
                Arg::Integer(i64::from(packet.amount)),
                Arg::Integer(i64::from(impact.kind.school())),
            ]),
        )?;
    }
    Ok(())
}
