//! Front-end policy for direct, single-act Playtest sessions.
//!
//! Playtest uses the campaign simulation, supplies, route and mission gates. Its differences are
//! entry presentation, the Act-One boundary, and an isolated sector-entry checkpoint filename.
//! Keeping those choices here prevents build, retry, save and completion routes from drifting.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

use crate::ui::PlayMode;

pub const PLAYTEST_ACT: u8 = 1;
pub const PLAYTEST_CHECKPOINT_FILE: &str = "playtest-act1-checkpoint.v1.json";

pub fn uses_campaign(mode: PlayMode) -> bool {
    matches!(mode, PlayMode::Kampagne | PlayMode::Playtest)
}

pub fn skips_act_card(mode: PlayMode) -> bool {
    mode == PlayMode::Playtest
}

/// Consume Playtest's successful campaign ActResult as completion, never as permission to build
/// Act Two. The ordinary campaign remains responsible for its complete multi-act flow.
pub fn completion_enters_next_act(mode: PlayMode) -> bool {
    mode == PlayMode::Kampagne
}

/// Apply to a selected manifest and a loaded checkpoint before replacing the live session.
pub fn validate_act(mode: PlayMode, act: u8) -> Result<()> {
    ensure!(
        mode != PlayMode::Playtest || act == PLAYTEST_ACT,
        "Playtest runs Act One; selected content is Act {act}"
    );
    Ok(())
}

/// Both modes keep the existing campaign save format and act-entrance semantics. Different
/// filenames prevent Playtest from overwriting or implicitly resuming an ordinary campaign.
/// The caller still selects/validates the correct authored manifest before save/load/build.
pub fn checkpoint_path(mode: PlayMode) -> Result<PathBuf> {
    checkpoint_path_from(mode, &crate::campaign_save::default_path()?)
}

fn checkpoint_path_from(mode: PlayMode, campaign_path: &Path) -> Result<PathBuf> {
    ensure!(
        uses_campaign(mode),
        "this mode does not own a campaign checkpoint"
    );
    Ok(if mode == PlayMode::Playtest {
        campaign_path.with_file_name(PLAYTEST_CHECKPOINT_FILE)
    } else {
        campaign_path.to_path_buf()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_and_direct_act_one_share_simulation_but_not_transition_policy() {
        assert!(uses_campaign(PlayMode::Kampagne));
        assert!(uses_campaign(PlayMode::Playtest));
        assert!(!skips_act_card(PlayMode::Kampagne));
        assert!(skips_act_card(PlayMode::Playtest));
        assert!(completion_enters_next_act(PlayMode::Kampagne));
        assert!(!completion_enters_next_act(PlayMode::Playtest));
    }

    #[test]
    fn development_and_multiplayer_modes_do_not_gain_campaign_authority() {
        for mode in [
            PlayMode::Garage,
            PlayMode::Gefecht,
            PlayMode::Koop,
            PlayMode::TestAct2,
            PlayMode::TestAct3,
        ] {
            assert!(!uses_campaign(mode), "unexpected campaign mode: {mode:?}");
            assert!(!skips_act_card(mode));
            assert!(!completion_enters_next_act(mode));
            assert!(checkpoint_path_from(mode, Path::new("/state/campaign.json")).is_err());
        }
    }

    #[test]
    fn playtest_rejects_wrong_act_before_manifest_or_checkpoint_installation() {
        assert!(validate_act(PlayMode::Playtest, PLAYTEST_ACT).is_ok());
        for act in [0, 2, 3, u8::MAX] {
            assert!(validate_act(PlayMode::Playtest, act).is_err());
        }
        for act in 1..=3 {
            assert!(validate_act(PlayMode::Kampagne, act).is_ok());
        }
    }

    #[test]
    fn playtest_checkpoint_is_a_sibling_and_cannot_replace_the_campaign_save() {
        for campaign in [
            Path::new("/state/panzerrechnersystem/campaign-checkpoint.v1.json"),
            Path::new("/home/owner/.local/state/panzerrechnersystem/campaign-checkpoint.v1.json"),
            Path::new(
                "/work tree/artifacts/shader-state/panzerrechnersystem/campaign-checkpoint.v1.json",
            ),
        ] {
            let normal = checkpoint_path_from(PlayMode::Kampagne, campaign).unwrap();
            let playtest = checkpoint_path_from(PlayMode::Playtest, campaign).unwrap();
            assert_eq!(normal, campaign);
            assert_ne!(playtest, normal);
            assert_eq!(playtest.parent(), normal.parent());
            assert_eq!(playtest.file_name().unwrap(), PLAYTEST_CHECKPOINT_FILE);
        }
    }
}
