use std::{borrow::Cow, mem, sync::Arc};

use bathbot_macros::command;
use bathbot_model::Rarity;
use bathbot_util::{
    constants::{GENERAL_ISSUE, OSEKAI_ISSUE, OSU_API_ISSUE},
    matcher, IntHasher, MessageBuilder,
};
use eyre::{Report, Result, WrapErr};
use hashbrown::HashMap;
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use plotters::prelude::*;
use rkyv::{Deserialize, Infallible};
use rosu_v2::{
    prelude::{MedalCompact, OsuError},
    request::UserId,
};
use time::OffsetDateTime;

use crate::{
    commands::osu::{require_link, user_not_found},
    core::commands::CommandOrigin,
    embeds::{EmbedData, MedalStatsEmbed},
    manager::redis::{osu::UserArgs, RedisData},
    util::Monthly,
    Context,
};

use super::MedalStats;

#[command]
#[desc("Display medal stats for a user")]
#[usage("[username]")]
#[examples("badewanne3", r#""im a fancy lad""#)]
#[alias("ms")]
#[group(AllModes)]
async fn prefix_medalstats(ctx: Arc<Context>, msg: &Message, mut args: Args<'_>) -> Result<()> {
    let args = match args.next() {
        Some(arg) => match matcher::get_mention_user(arg) {
            Some(id) => MedalStats {
                name: None,
                discord: Some(id),
            },
            None => MedalStats {
                name: Some(Cow::Borrowed(arg)),
                discord: None,
            },
        },
        None => MedalStats::default(),
    };

    stats(ctx, msg.into(), args).await
}

pub(super) async fn stats(
    ctx: Arc<Context>,
    orig: CommandOrigin<'_>,
    args: MedalStats<'_>,
) -> Result<()> {
    let user_id = match user_id!(ctx, orig, args) {
        Some(user_id) => user_id,
        None => match ctx.user_config().osu_id(orig.user_id()?).await {
            Ok(Some(user_id)) => UserId::Id(user_id),
            Ok(None) => return require_link(&ctx, &orig).await,
            Err(err) => {
                let _ = orig.error(&ctx, GENERAL_ISSUE).await;

                return Err(err);
            }
        },
    };

    let user_args = UserArgs::rosu_id(&ctx, &user_id).await;
    let user_fut = ctx.redis().osu_user(user_args);
    let medals_fut = ctx.redis().medals();
    let ranking_fut = ctx.redis().osekai_ranking::<Rarity>();

    let (mut user, all_medals, ranking) = match tokio::join!(user_fut, medals_fut, ranking_fut) {
        (Ok(user), Ok(medals), Ok(ranking)) => (user, medals, Some(ranking)),
        (Err(OsuError::NotFound), ..) => {
            let content = user_not_found(&ctx, user_id).await;

            return orig.error(&ctx, content).await;
        }
        (_, Err(err), _) => {
            let _ = orig.error(&ctx, OSEKAI_ISSUE).await;

            return Err(err.wrap_err("failed to get cached medals"));
        }
        (Err(err), ..) => {
            let _ = orig.error(&ctx, OSU_API_ISSUE).await;
            let report = Report::new(err).wrap_err("failed to get user");

            return Err(report);
        }
        (Ok(user), Ok(medals), Err(err)) => {
            warn!("{:?}", err.wrap_err("Failed to get cached rarity ranking"));

            (user, medals, None)
        }
    };

    let mut medals = match user {
        RedisData::Original(ref mut user) => mem::take(&mut user.medals),
        RedisData::Archived(ref user) => user.medals.deserialize(&mut Infallible).unwrap(),
    };

    medals.sort_unstable_by_key(|medal| medal.achieved_at);

    let graph = match graph(&medals, W, H) {
        Ok(bytes_option) => bytes_option,
        Err(err) => {
            warn!("{:?}", err.wrap_err("failed to create graph"));

            None
        }
    };

    let all_medals: HashMap<_, _, IntHasher> = match all_medals {
        RedisData::Original(all_medals) => all_medals
            .into_iter()
            .map(|entry| (entry.medal_id, (entry.name, entry.grouping)))
            .collect(),
        RedisData::Archived(all_medals) => all_medals
            .iter()
            .map(|entry| {
                let name = entry.name.deserialize(&mut Infallible).unwrap();
                let grouping = entry.grouping.deserialize(&mut Infallible).unwrap();

                (entry.medal_id, (name, grouping))
            })
            .collect(),
    };

    let rarest = ranking.and_then(|ranking| {
        let ranking: HashMap<_, _, IntHasher> = match ranking {
            RedisData::Original(ranking) => ranking
                .iter()
                .map(|entry| (entry.medal_id, entry.possession_percent))
                .collect(),
            RedisData::Archived(ranking) => ranking
                .iter()
                .map(|entry| (entry.medal_id, entry.possession_percent))
                .collect(),
        };

        medals
            .iter()
            .min_by_key(|medal| {
                ranking
                    .get(&medal.medal_id)
                    .map_or(i32::MAX, |&perc| (perc * 10_000.0) as i32)
            })
            .copied()
    });

    let embed = MedalStatsEmbed::new(&user, &medals, &all_medals, rarest, graph.is_some()).build();
    let mut builder = MessageBuilder::new().embed(embed);

    if let Some(graph) = graph {
        builder = builder.attachment("medal_graph.png", graph);
    }

    orig.create_message(&ctx, &builder).await?;

    Ok(())
}

const W: u32 = 1350;
const H: u32 = 350;

pub fn graph(medals: &[MedalCompact], w: u32, h: u32) -> Result<Option<Vec<u8>>> {
    let len = (w * h) as usize;
    let mut buf = vec![0; len * 3]; // PIXEL_SIZE = 3

    {
        let root = BitMapBackend::with_buffer(&mut buf, (w, h)).into_drawing_area();
        let background = RGBColor(19, 43, 33);
        root.fill(&background)
            .wrap_err("failed to fill background")?;

        if medals.is_empty() {
            return Ok(None);
        }

        let first = medals.first().unwrap().achieved_at;
        let last = medals.last().unwrap().achieved_at;

        let style: fn(RGBColor) -> ShapeStyle = |color| ShapeStyle {
            color: color.to_rgba(),
            filled: false,
            stroke_width: 1,
        };

        let mut chart = ChartBuilder::on(&root)
            .margin_right(22)
            .caption("Medal history", ("sans-serif", 30, &WHITE))
            .x_label_area_size(30)
            .y_label_area_size(45)
            .build_cartesian_2d(Monthly(first..last), 0..medals.len())
            .wrap_err("failed to build chart")?;

        // Mesh and labels
        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_labels(10)
            .x_label_formatter(&|d| format!("{}-{}", d.year(), d.month() as u8))
            .label_style(("sans-serif", 20, &WHITE))
            .bold_line_style(WHITE.mix(0.3))
            .axis_style(RGBColor(7, 18, 14))
            .axis_desc_style(("sans-serif", 16, FontStyle::Bold, &WHITE))
            .draw()
            .wrap_err("failed to draw mesh and labels")?;

        // Draw area
        let area_style = RGBColor(2, 186, 213).mix(0.7).filled();
        let border_style = style(RGBColor(0, 208, 138)).stroke_width(3);
        let counter = MedalCounter::new(medals);
        let series = AreaSeries::new(counter, 0, area_style).border_style(border_style);
        chart.draw_series(series).wrap_err("failed to draw area")?;
    }

    // Encode buf to png
    let mut png_bytes: Vec<u8> = Vec::with_capacity(len);
    let png_encoder = PngEncoder::new(&mut png_bytes);

    png_encoder
        .write_image(&buf, w, h, ColorType::Rgb8)
        .wrap_err("failed to encode image")?;

    Ok(Some(png_bytes))
}

struct MedalCounter<'m> {
    count: usize,
    medals: &'m [MedalCompact],
}

impl<'m> MedalCounter<'m> {
    fn new(medals: &'m [MedalCompact]) -> Self {
        Self { count: 0, medals }
    }
}

impl Iterator for MedalCounter<'_> {
    type Item = (OffsetDateTime, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let date = self.medals.first()?.achieved_at;
        self.count += 1;
        self.medals = &self.medals[1..];

        Some((date, self.count))
    }
}