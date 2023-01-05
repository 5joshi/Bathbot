use std::sync::Arc;

use bathbot_model::OsuTrackerModsEntry;
use bathbot_util::constants::OSUTRACKER_ISSUE;
use eyre::Result;
use rkyv::{Deserialize, Infallible};

use crate::{
    core::Context,
    manager::redis::RedisData,
    pagination::OsuTrackerModsPagination,
    util::{interaction::InteractionCommand, InteractionCommandExt},
};

pub(super) async fn mods(ctx: Arc<Context>, mut command: InteractionCommand) -> Result<()> {
    let counts: Vec<OsuTrackerModsEntry> = match ctx.redis().osutracker_stats().await {
        Ok(RedisData::Original(stats)) => stats.user.mods_count,
        Ok(RedisData::Archived(stats)) => {
            stats.user.mods_count.deserialize(&mut Infallible).unwrap()
        }
        Err(err) => {
            let _ = command.error(&ctx, OSUTRACKER_ISSUE).await;

            return Err(err.wrap_err("failed to get cached osutracker stats"));
        }
    };

    OsuTrackerModsPagination::builder(counts)
        .start_by_update()
        .defer_components()
        .start(ctx, (&mut command).into())
        .await
}