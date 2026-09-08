//! Tutorial acknowledgement admission under bounded writer backpressure.

use super::{ActiveGameplayNetwork, RuntimeGameplayCoordinator, WorldWriterCommand};
use crate::test_network::TestError;
use solarity_ui::{UiTutorialAction, UiTutorialState};

#[test]
fn water_tutorial_acknowledgements_retain_order_under_backpressure() -> Result<(), TestError> {
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let (commands, mut writer) = tokio::sync::mpsc::channel(1);
    commands.try_send(WorldWriterCommand::ActiveMover(8))?;
    let mut gameplay = RuntimeGameplayCoordinator::new();
    gameplay.active = Some(ActiveGameplayNetwork {
        receiver,
        commands,
        task: runtime.spawn(std::future::pending()),
    });
    let state = UiTutorialState::default();
    state.replace_flags(&[0; 32]);
    state.flag(27);
    state.clear();
    state.reset();
    assert!(!gameplay.send_tutorial_action(state.pending_action().ok_or("pending flag")?)?);
    assert_eq!(state.pending_action(), Some(UiTutorialAction::Flag(27)));
    assert_eq!(writer.try_recv()?, WorldWriterCommand::ActiveMover(8));
    for action in [
        UiTutorialAction::Flag(27),
        UiTutorialAction::Clear,
        UiTutorialAction::Reset,
    ] {
        assert_eq!(state.pending_action(), Some(action));
        assert!(gameplay.send_tutorial_action(action)?);
        state.accept_action();
        if let Some(next) = state.pending_action() {
            assert!(!gameplay.send_tutorial_action(next)?);
            assert_eq!(state.pending_action(), Some(next));
        }
        assert_eq!(writer.try_recv()?, WorldWriterCommand::Tutorial(action));
    }
    assert_eq!(state.pending_action(), None);
    let actions = [
        solarity_ui::UiPlayerDeathAction::ReleaseSpirit { automatic: false },
        solarity_ui::UiPlayerDeathAction::SelfResurrect,
        solarity_ui::UiPlayerDeathAction::ResurrectionResponse {
            guid: 7,
            accept: true,
        },
        solarity_ui::UiPlayerDeathAction::ReclaimCorpse { guid: 8 },
    ];
    for action in actions {
        assert!(gameplay.send_player_death_action(action)?);
        assert!(!gameplay.send_player_death_action(actions[0])?);
        assert_eq!(writer.try_recv()?, WorldWriterCommand::PlayerDeath(action));
        assert!(writer.try_recv().is_err());
    }
    let guid = 0x1234567800000009;
    assert_eq!(gameplay.player_ui.names.request_for_offer(guid), None);
    assert_eq!(gameplay.player_ui.names.request_for_offer(guid), None);
    gameplay
        .active
        .as_ref()
        .ok_or("active writer")?
        .commands
        .try_send(WorldWriterCommand::ActiveMover(8))?;
    gameplay.send_player_name_queries()?;
    assert_eq!(gameplay.player_ui.names.pending_request(), Some(guid));
    assert_eq!(writer.try_recv()?, WorldWriterCommand::ActiveMover(8));
    gameplay.send_player_name_queries()?;
    assert_eq!(gameplay.player_ui.names.pending_request(), None);
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::PlayerNameQuery(guid)
    );
    assert!(
        writer.try_recv().is_err(),
        "duplicate callbacks share one query"
    );
    let queries = [
        super::player_corpse::CorpseQuery::Location,
        super::player_corpse::CorpseQuery::Transport(9),
    ];
    gameplay.player_ui.corpse.queries.extend(queries);
    gameplay.send_corpse_queries()?;
    assert_eq!(gameplay.player_ui.corpse.queries.front(), Some(&queries[1]));
    gameplay.send_corpse_queries()?;
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::CorpseQuery(queries[0])
    );
    gameplay.send_corpse_queries()?;
    assert!(gameplay.player_ui.corpse.queries.is_empty());
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::CorpseQuery(queries[1])
    );
    Ok(())
}
