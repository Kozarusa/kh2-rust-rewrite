//! Plugin bundles. The game's plugins are grouped by domain; `GamePlugins` adds the bundles in
//! the order the plugins were registered before the split into bundles, which keeps the schedules'
//! tie-breaking between unordered systems unchanged. Registration order, front to back:
//! schedule, assets, sora, camera, skel, combat, enemy, enemy_ai, hud, fx, harness, audio,
//! abilities, drive, limit, reaction_cmd, clash, player_reaction, magic, texanim, progress, prize,
//! menu, menu_abilities, items, party, struggle, summon, sound_test, asset_viewer, area, area_jump,
//! npc, tutorial, intro, journal, struggle menu, shop.
//! `AssetsPlugin` must come first among the rest (it provides `Disc`, which `SoraPlugin` reads when
//! it builds). `progress::ensure` lets earlier plugins that read `Progress` while they build insert
//! it first, so the place of `ProgressPlugin` does not matter.
//! New plugins: add them to the bundle of their domain; a bundle has no size limit.

use std::path::PathBuf;

use bevy::app::{PluginGroup, PluginGroupBuilder};

use crate::{
    abilities, area, area_jump, asset_viewer, assets, audio, camera, clash, combat, drive, enemy, enemy_ai, fx, harness, hud, intro, items,
    limit, magic, menu, menu_abilities, menu_ending, menu_journal, menu_shop, menu_struggle, npc, party, player_reaction, prize, progress,
    reaction_cmd, schedule, skel, sora, sound_test, struggle, summon, texanim,
};

/// Core: the schedule and the disc.
pub struct CorePlugins {
    pub iso: PathBuf,
    pub start: String,
}

impl PluginGroup for CorePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(schedule::SchedulePlugin).add(assets::AssetsPlugin { iso: self.iso, start: self.start })
    }
}

/// The player, camera, skeletons, combat core and enemies.
pub struct ActorPlugins;

impl PluginGroup for ActorPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(sora::SoraPlugin)
            .add(camera::CameraPlugin)
            .add(skel::SkelPlugin)
            .add(combat::CombatPlugin)
            .add(enemy::EnemyPlugin)
            .add(enemy_ai::EnemyAiPlugin)
    }
}

/// HUD, effects, test harness and audio.
pub struct PresentationPlugins;

impl PluginGroup for PresentationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(hud::HudPlugin).add(fx::FxPlugin).add(harness::HarnessPlugin).add(audio::AudioPlugin)
    }
}

/// Combat extensions: abilities, drive, limits, reactions, magic.
pub struct CombatPlugins;

impl PluginGroup for CombatPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(abilities::AbilitiesPlugin)
            .add(drive::DrivePlugin)
            .add(limit::LimitPlugin)
            .add(reaction_cmd::ReactionPlugin)
            .add(clash::ClashPlugin)
            .add(player_reaction::PlayerReactionPlugin)
            .add(magic::MagicPlugin)
            .add(texanim::TexAnimPlugin)
    }
}

/// Progress, menus, items, party, summons and the debug viewers.
pub struct MenuPlugins;

impl PluginGroup for MenuPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(progress::ProgressPlugin)
            .add(prize::PrizePlugin)
            .add(menu::MenuPlugin)
            .add(menu_abilities::MenuAbilitiesPlugin)
            .add(items::ItemsPlugin)
            .add(party::PartyPlugin)
            .add(struggle::StrugglePlugin)
            .add(summon::SummonPlugin)
            .add(sound_test::SoundTestPlugin)
            .add(asset_viewer::AssetViewerPlugin)
    }
}

/// World and events: areas, NPCs, the tutorial and the intro.
pub struct WorldPlugins;

impl PluginGroup for WorldPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(area::AreaPlugin)
            .add(area_jump::AreaJumpPlugin)
            .add(npc::NpcPlugin)
            .add(hud::hud_msgwin::hud_tutorial::TutorialPlugin)
            .add(intro::IntroPlugin)
    }
}

/// Later menu pages: journal, struggle menu, shop.
pub struct MenuPagePlugins;

impl PluginGroup for MenuPagePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(menu_journal::JournalPlugin)
            .add(menu_struggle::StruggleMenuPlugin)
            .add(menu_shop::ShopPlugin)
            .add(menu_ending::EndingPlugin)
    }
}

/// Every game plugin, in registration order.
pub struct GamePlugins {
    pub iso: PathBuf,
    pub start: String,
}

impl PluginGroup for GamePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(CorePlugins { iso: self.iso, start: self.start })
            .add_group(ActorPlugins)
            .add_group(PresentationPlugins)
            .add_group(CombatPlugins)
            .add_group(MenuPlugins)
            .add_group(WorldPlugins)
            .add_group(MenuPagePlugins)
    }
}
