use super::prepare_score;
use crate::{
    arguments::{Args, SimulateNameArgs},
    embeds::{EmbedData, SimulateEmbed},
    util::{
        constants::{GENERAL_ISSUE, OSU_API_ISSUE},
        MessageExt,
    },
    BotResult, Context,
};

use rosu_v2::prelude::{GameMode, OsuError};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use twilight_model::channel::Message;

async fn simulate_recent_main(
    mode: GameMode,
    ctx: Arc<Context>,
    msg: &Message,
    args: Args<'_>,
    num: Option<usize>,
) -> BotResult<()> {
    let mut args = match SimulateNameArgs::new(&ctx, args) {
        Ok(args) => args,
        Err(err_msg) => return msg.error(&ctx, err_msg).await,
    };

    let name = match args.name.take().or_else(|| ctx.get_link(msg.author.id.0)) {
        Some(name) => name,
        None => return super::require_link(&ctx, msg).await,
    };

    let limit = num.map_or(1, |n| n + (n == 0) as usize);

    if limit > 50 {
        let content = "Recent history goes only 50 scores back.";

        return msg.error(&ctx, content).await;
    }

    // Retrieve the recent score
    let scores_fut = ctx
        .osu()
        .user_scores(&name)
        .recent()
        .mode(mode)
        .include_fails(true)
        .limit(limit);

    let mut score = match scores_fut.await {
        Ok(scores) if scores.is_empty() => {
            let content = format!(
                "No recent {}plays found for user `{}`",
                match mode {
                    GameMode::STD => "",
                    GameMode::TKO => "taiko ",
                    GameMode::CTB => "ctb ",
                    GameMode::MNA => "mania ",
                },
                name
            );

            return msg.error(&ctx, content).await;
        }
        Ok(scores) if scores.len() < limit => {
            let content = format!(
                "There are only {} many scores in `{}`'{} recent history.",
                scores.len(),
                name,
                if name.ends_with('s') { "" } else { "s" }
            );

            return msg.error(&ctx, content).await;
        }
        Ok(mut scores) => match scores.pop() {
            Some(mut score) => match prepare_score(&ctx, &mut score).await {
                Ok(_) => score,
                Err(why) => {
                    let _ = msg.error(&ctx, OSU_API_ISSUE).await;

                    return Err(why.into());
                }
            },
            None => {
                let content = format!("No recent plays found for user `{}`", name);

                return msg.error(&ctx, content).await;
            }
        },
        Err(OsuError::NotFound) => {
            let content = format!("User `{}` was not found", name);

            return msg.error(&ctx, content).await;
        }
        Err(why) => {
            let _ = msg.error(&ctx, OSU_API_ISSUE).await;

            return Err(why.into());
        }
    };

    let map = score.map.take().unwrap();
    let mapset = score.mapset.take().unwrap();

    // Accumulate all necessary data
    let data = match SimulateEmbed::new(Some(score), &map, &mapset, args.into()).await {
        Ok(data) => data,
        Err(why) => {
            let _ = msg.error(&ctx, GENERAL_ISSUE).await;

            return Err(why);
        }
    };

    // Creating the embed
    let embed = data.build().build()?;

    let response = ctx
        .http
        .create_message(msg.channel_id)
        .content("Simulated score:")?
        .embed(embed)?
        .await?;

    ctx.store_msg(response.id);
    response.reaction_delete(&ctx, msg.author.id);

    // Store map in DB
    if let Err(why) = ctx.psql().insert_beatmap(&map).await {
        unwind_error!(
            warn,
            why,
            "Error while storing simulate recent map in DB: {}"
        )
    }

    // Set map on garbage collection list if unranked
    let gb = ctx.map_garbage_collector(&map);

    // Minimize embed after delay
    tokio::spawn(async move {
        gb.execute(&ctx).await;
        sleep(Duration::from_secs(45)).await;

        if !ctx.remove_msg(response.id) {
            return;
        }

        let embed = data.minimize().build().unwrap();

        let embed_update = ctx
            .http
            .update_message(response.channel_id, response.id)
            .embed(embed)
            .unwrap();

        if let Err(why) = embed_update.await {
            unwind_error!(warn, why, "Error minimizing simulaterecent msg: {}");
        }
    });

    Ok(())
}

#[command]
#[short_desc("Unchoke a user's most recent play")]
#[long_desc(
    "Unchoke a user's most recent play.\n\
    To get a previous recent score, you can add a number right after the command,\n\
    e.g. `sr42 badewanne3` to get the 42nd most recent score."
)]
#[usage(
    "[username] [+mods] [-a acc%] [-c combo] [-300 #300s] [-100 #100s] [-50 #50s] [-m #misses]"
)]
#[example("badewanne3 +hr -a 99.3 -300 1422 -m 1")]
#[aliases("sr")]
pub async fn simulaterecent(
    ctx: Arc<Context>,
    msg: &Message,
    args: Args,
    num: Option<usize>,
) -> BotResult<()> {
    simulate_recent_main(GameMode::STD, ctx, msg, args, num).await
}

#[command]
#[short_desc("Display a perfect play on a user's most recently played mania map")]
#[long_desc(
    "Display a perfect play on a user's most recently played mania map.\n\
    To get a previous recent map, you can add a number right after the command,\n\
    e.g. `srm42 badewanne3` to get the 42nd most recent map."
)]
#[usage("[username] [+mods] [-s score]")]
#[example("badewanne3 +dt -s 895000")]
#[aliases("srm")]
pub async fn simulaterecentmania(
    ctx: Arc<Context>,
    msg: &Message,
    args: Args,
    num: Option<usize>,
) -> BotResult<()> {
    simulate_recent_main(GameMode::MNA, ctx, msg, args, num).await
}

#[command]
#[short_desc("Unchoke a user's most recent taiko play")]
#[long_desc(
    "Unchoke a user's most recent taiko play.\n\
    To get a previous recent score, you can add a number right after the command,\n\
    e.g. `srt42 badewanne3` to get the 42nd most recent score."
)]
#[usage("[username] [+mods] [-a acc%] [-c combo] [-m #misses]")]
#[example("badewanne3 +hr -a 99.3 -m 1")]
#[aliases("srt")]
pub async fn simulaterecenttaiko(
    ctx: Arc<Context>,
    msg: &Message,
    args: Args,
    num: Option<usize>,
) -> BotResult<()> {
    simulate_recent_main(GameMode::TKO, ctx, msg, args, num).await
}

#[command]
#[short_desc("Unchoke a user's most recent ctb play")]
#[long_desc(
    "Unchoke a user's most recent ctb play.\n\
    To get a previous recent score, you can add a number right after the command,\n\
    e.g. `src42 badewanne3` to get the 42nd most recent score."
)]
#[usage(
    "[username] [+mods] [-a acc%] [-c combo] [-300 #fruits] [-100 #droplets] [-50 #tiny droplets] [-m #misses]"
)]
#[example("badewanne3 +hr -a 99.3 -300 1422 -m 1")]
#[aliases("src")]
pub async fn simulaterecentctb(
    ctx: Arc<Context>,
    msg: &Message,
    args: Args,
    num: Option<usize>,
) -> BotResult<()> {
    simulate_recent_main(GameMode::CTB, ctx, msg, args, num).await
}
