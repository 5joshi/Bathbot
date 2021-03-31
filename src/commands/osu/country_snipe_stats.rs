use super::request_user;
use crate::{
    arguments::Args,
    custom_client::SnipeCountryPlayer,
    embeds::{CountrySnipeStatsEmbed, EmbedData},
    util::{
        constants::{HUISMETBENEN_ISSUE, OSU_API_ISSUE},
        MessageExt, SNIPE_COUNTRIES,
    },
    BotResult, Context,
};

use image::{png::PngEncoder, ColorType};
use plotters::prelude::*;
use rosu_v2::prelude::{GameMode, OsuError};
use std::{cmp::Ordering::Equal, sync::Arc};
use twilight_model::channel::Message;

#[command]
#[short_desc("Snipe / #1 count related stats for a country")]
#[long_desc(
    "Some snipe / #1 count related stats for a country.\n\
    As argument, provide either `global`, or a country acronym, e.g. `be`.\n\
    If no country is specified, I will take the country of the linked user.\n\
    All data originates from [Mr Helix](https://osu.ppy.sh/users/2330619)'s \
    website [huismetbenen](https://snipe.huismetbenen.nl/)."
)]
#[usage("[country acronym]")]
#[example("fr", "global")]
#[aliases("css")]
#[bucket("snipe")]
async fn countrysnipestats(ctx: Arc<Context>, msg: &Message, mut args: Args) -> BotResult<()> {
    let country = match args.next() {
        Some(arg) => match arg {
            "global" | "world" => String::from("global"),
            _ => {
                if arg.len() != 2 || arg.chars().count() != 2 {
                    let content = "The argument must be a country acronym of length two, e.g. `fr`";
                    return msg.error(&ctx, content).await;
                }

                let arg = arg.to_uppercase();

                if !SNIPE_COUNTRIES.contains_key(arg.as_str()) {
                    let content = "That country acronym is not supported :(";
                    return msg.error(&ctx, content).await;
                }

                arg
            }
        },
        None => match ctx.get_link(msg.author.id.0) {
            Some(name) => {
                let user = match request_user(&ctx, &name, Some(GameMode::STD)).await {
                    Ok(user) => user,
                    Err(OsuError::NotFound) => {
                        let content = format!("User `{}` was not found", name);

                        return msg.error(&ctx, content).await;
                    }
                    Err(why) => {
                        let _ = msg.error(&ctx, OSU_API_ISSUE).await;

                        return Err(why.into());
                    }
                };

                if SNIPE_COUNTRIES.contains_key(user.country_code.as_str()) {
                    user.country_code.to_owned()
                } else {
                    let content = format!(
                        "`{}`'s country {} is not supported :(",
                        user.username, user.country_code
                    );

                    return msg.error(&ctx, content).await;
                }
            }
            None => {
                let content =
                    "Since you're not linked, you must specify a country acronym, e.g. `fr`";

                return msg.error(&ctx, content).await;
            }
        },
    };

    let client = &ctx.clients.custom;

    let (players, statistics) = {
        match tokio::try_join!(
            client.get_snipe_country(&country),
            client.get_country_statistics(&country),
        ) {
            Ok((players, statistics)) => (players, statistics),
            Err(why) => {
                let _ = msg.error(&ctx, HUISMETBENEN_ISSUE).await;

                return Err(why.into());
            }
        }
    };

    let graph = match graphs(&players) {
        Ok(graph_option) => Some(graph_option),
        Err(why) => {
            unwind_error!(warn, why, "Error while creating snipe country graph: {}");

            None
        }
    };

    let country = SNIPE_COUNTRIES.get(country.as_str());
    let data = CountrySnipeStatsEmbed::new(country, statistics);

    // Sending the embed
    let embed = data.into_builder().build();
    let m = ctx.http.create_message(msg.channel_id).embed(embed)?;

    if let Some(graph) = graph {
        m.attachment("stats_graph.png", graph).await?
    } else {
        m.await?
    };

    Ok(())
}

const W: u32 = 1350;
const H: u32 = 350;

fn graphs(players: &[SnipeCountryPlayer]) -> BotResult<Vec<u8>> {
    static LEN: usize = W as usize * H as usize;
    let mut pp: Vec<_> = players
        .iter()
        .map(|player| (&player.username, player.pp))
        .collect();
    pp.sort_unstable_by(|(_, pp1), (_, pp2)| pp2.partial_cmp(pp1).unwrap_or(Equal));
    pp.truncate(11);
    let mut count: Vec<_> = players
        .iter()
        .map(|player| (&player.username, player.count_first as i32))
        .collect();
    count.sort_unstable_by(|(_, c1), (_, c2)| c2.cmp(c1));
    count.truncate(11);
    let pp_max = pp
        .iter()
        .map(|(_, n)| *n)
        .fold(0.0_f32, |max, curr| max.max(curr));
    let count_max = count
        .iter()
        .map(|(_, n)| *n)
        .fold(0, |max, curr| max.max(curr));
    let mut buf = vec![0; LEN * 3]; // PIXEL_SIZE = 3
    {
        let root = BitMapBackend::with_buffer(&mut buf, (W, H)).into_drawing_area();
        root.fill(&WHITE)?;
        let (left, right) = root.split_horizontally(W / 2);
        let mut chart = ChartBuilder::on(&left)
            .x_label_area_size(30)
            .y_label_area_size(60)
            .margin_right(15)
            .caption("Weighted pp from #1s", ("sans-serif", 30))
            .build_cartesian_2d(0..pp.len() - 1, 0.0..pp_max)?;

        // Mesh and labels
        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_label_offset(30)
            .x_label_style(("sans-serif", 10))
            .x_label_formatter(&|idx| {
                if *idx < 10 {
                    pp[*idx].0.to_string()
                } else {
                    String::new()
                }
            })
            .draw()?;

        // Histogram bars
        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.mix(0.5).filled())
                .data(
                    pp.iter()
                        .take(10)
                        .enumerate()
                        .map(|(idx, (_, n))| (idx, *n)),
                ),
        )?;

        // Count graph
        let mut chart = ChartBuilder::on(&right)
            .x_label_area_size(30)
            .y_label_area_size(35)
            .margin_right(15)
            .caption("#1 Count", ("sans-serif", 30))
            .build_cartesian_2d(0..count.len() - 1, 0..count_max)?;

        // Mesh and labels
        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_label_offset(30)
            .x_label_style(("sans-serif", 10))
            .x_label_formatter(&|idx| {
                if *idx < 10 {
                    count[*idx].0.to_string()
                } else {
                    String::new()
                }
            })
            .draw()?;

        // Histogram bars
        chart.draw_series(
            Histogram::vertical(&chart)
                .style(RED.mix(0.5).filled())
                .data(
                    count
                        .iter()
                        .take(10)
                        .enumerate()
                        .map(|(idx, (_, n))| (idx, *n)),
                ),
        )?;
    }

    // Encode buf to png
    let mut png_bytes: Vec<u8> = Vec::with_capacity(LEN);
    let png_encoder = PngEncoder::new(&mut png_bytes);
    png_encoder.encode(&buf, W, H, ColorType::Rgb8)?;
    Ok(png_bytes)
}
