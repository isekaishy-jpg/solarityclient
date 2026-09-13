//! Test-only live-session adapter to the production Lua/network entry path.

use super::*;
use std::{error::Error, path::Path};

impl ClientServices {
    pub(crate) fn diagnostic_submit_login(
        &mut self,
        account: &str,
        password: &str,
    ) -> Result<(), Box<dyn Error>> {
        if self.glue.current_screen() != "login" {
            return Err("expected login screen".into());
        }
        self.glue
            .bundle()
            .lua()
            .load("local account, password = ...; DefaultServerLogin(account, password)")
            .call::<()>((account, password))?;
        self.glue_ui_dirty = true;
        Ok(())
    }

    pub(crate) fn diagnostic_advance_login(
        &mut self,
        character: &str,
        realm_requested: &mut bool,
        character_requested: &mut bool,
    ) -> Result<bool, Box<dyn Error>> {
        if !*realm_requested && self.world_state() == RuntimeWorldState::Idle {
            let selected: bool = self
                .glue
                .bundle()
                .lua()
                .load(
                    r#"
                if GetNumRealms() ~= 1 then return false end
                for category = 1, select('#', GetRealmCategories()) do
                    if GetNumRealms(category) == 1 then ChangeRealm(category, 1); return true end
                end
                return false
            "#,
                )
                .eval()?;
            *realm_requested = selected;
        }
        if !*character_requested && self.world_state() == RuntimeWorldState::CharacterSelection {
            let selected: i32 = self
                .glue
                .bundle()
                .lua()
                .load(
                    r#"
                local target = ...
                local count = GetNumCharacters()
                if count == 0 then return 0 end
                local selected, matches = nil, 0
                for index = 1, count do
                    if string.lower(GetCharacterInfo(index)) == string.lower(target) then
                        selected, matches = index, matches + 1
                    end
                end
                if matches ~= 1 then return -1 end
                SelectCharacter(selected)
                EnterWorld()
                return 1
            "#,
                )
                .call(character)?;
            if selected < 0 {
                return Err("requested character is missing or ambiguous".into());
            }
            *character_requested = selected == 1;
            if *character_requested {
                tracing::info!(character, "live diagnostic selected character");
            }
        }
        Ok(self.gameplay.world().is_some()
            && self.loading_screen.is_none()
            && self.world_ui.is_some())
    }

    pub(crate) fn diagnostic_capture_scene(&mut self, path: &Path) -> Result<(), Box<dyn Error>> {
        tracing::info!(resident_tiles = self.terrain.resident_tile_count(),
            farclip = ?self.glue.cvar_number("farclip"),
            shadow_quality = ?self.glue.cvar_number("extShadowQuality"),
            environment_detail = ?self.glue.cvar_number("environmentDetail"),
            "live diagnostic scene ready");
        if let Some(frame) = &self.terrain_frame {
            frame.log_diagnostic_workload();
        }
        self.renderer.request_frame_capture()?;
        self.present_frame()?;
        let frame = self
            .renderer
            .take_captured_frame()?
            .ok_or("scene capture unavailable")?;
        glue_benchmark::write_capture(path, &frame)?;
        Ok(())
    }
}
