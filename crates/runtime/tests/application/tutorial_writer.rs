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
    ];
    for action in actions {
        assert!(gameplay.send_player_death_action(action)?);
        assert!(!gameplay.send_player_death_action(actions[0])?);
        assert_eq!(writer.try_recv()?, WorldWriterCommand::PlayerDeath(action));
        assert!(writer.try_recv().is_err());
    }
    Ok(())
}
