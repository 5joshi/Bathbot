use super::ErrorType;
use crate::{
    database::UserConfig,
    embeds::{EmbedData, TopEmbed},
    pagination::{Pagination, TopPagination},
    tracking::process_tracking,
    util::{
        constants::{
            common_literals::{DISCORD, MODE, NAME},
            GENERAL_ISSUE, OSU_API_ISSUE,
        },
        matcher, numbers, CowUtils, MessageExt,
    },
    Args, BotResult, CommandData, Context, Error, MessageBuilder, Name,
};

use eyre::Report;
use futures::future::TryFutureExt;
use rosu_v2::prelude::{GameMode, OsuError};
use std::{borrow::Cow, sync::Arc};
use twilight_model::{
    application::interaction::application_command::CommandDataOption, id::UserId,
};

pub(super) async fn _mapper(
    ctx: Arc<Context>,
    data: CommandData<'_>,
    args: MapperArgs,
) -> BotResult<()> {
    let MapperArgs { config, mapper } = args;
    let mode = config.mode.unwrap_or(GameMode::STD);

    let user = match config.into_username() {
        Some(name) => name,
        None => return super::require_link(&ctx, &data).await,
    };

    let mapper = mapper.cow_to_ascii_lowercase();

    // Retrieve the user and their top scores
    let user_fut = super::request_user(&ctx, &user, mode).map_err(From::from);
    let scores_fut = ctx
        .osu()
        .user_scores(user.as_str())
        .best()
        .mode(mode)
        .limit(100);

    let scores_fut = super::prepare_scores(&ctx, scores_fut);

    let (mut user, mut scores) = match tokio::try_join!(user_fut, scores_fut) {
        Ok((user, scores)) => (user, scores),
        Err(ErrorType::Osu(OsuError::NotFound)) => {
            let content = format!("User `{}` was not found", user);

            return data.error(&ctx, content).await;
        }
        Err(ErrorType::Osu(why)) => {
            let _ = data.error(&ctx, OSU_API_ISSUE).await;

            return Err(why.into());
        }
        Err(ErrorType::Bot(why)) => {
            let _ = data.error(&ctx, GENERAL_ISSUE).await;

            return Err(why);
        }
    };

    // Overwrite default mode
    user.mode = mode;

    // Process user and their top scores for tracking
    process_tracking(&ctx, mode, &mut scores, Some(&user)).await;

    let mut scores: Vec<_> = scores
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i + 1, s))
        .collect();

    scores.retain(|(_, score)| {
        let map = &score.map.as_ref().unwrap();
        let mapset = &score.mapset.as_ref().unwrap();

        //  Filter converts
        if map.mode != mode {
            return false;
        }

        // Either the version contains the mapper name (guest diff'd by mapper)
        // or the map is created by mapper name and not guest diff'd by someone else
        let version = map.version.to_lowercase();

        version.contains(mapper.as_ref())
            || (mapset.creator_name.to_lowercase().as_str() == mapper.as_ref()
                && !matcher::is_guest_diff(&version))
    });

    // Accumulate all necessary data
    let content = match mapper.as_ref() {
        "sotarks" => {
            let amount = scores.len();

            let mut content = format!(
                "I found {amount} Sotarks map{plural} in `{name}`'s top100, ",
                amount = amount,
                plural = if amount != 1 { "s" } else { "" },
                name = user.username,
            );

            let to_push = match amount {
                0 => "proud of you \\:)",
                1..=4 => "that's already too many...",
                5..=8 => "kinda sad \\:/",
                9..=15 => "pretty sad \\:(",
                16..=25 => "this is so sad \\:((",
                26..=35 => "you need to stop this",
                36..=49 => "you have a serious problem...",
                50 => "that's half. HALF.",
                51..=79 => "how do you sleep at night...",
                80..=89 => "so close to ultimate disaster...",
                90..=99 => "i'm not even mad, that's just impressive",
                100 => "you did it. \"Congrats\".",
                _ => "wait how did you do that",
            };

            content.push_str(to_push);

            content
        }
        _ => format!(
            "{} of `{}`'{} top score maps were mapped by `{}`",
            scores.len(),
            user.username,
            if user.username.ends_with('s') {
                ""
            } else {
                "s"
            },
            mapper
        ),
    };

    let builder = if scores.is_empty() {
        MessageBuilder::new().embed(content)
    } else {
        let pages = numbers::div_euclid(5, scores.len());
        let data = TopEmbed::new(&user, scores.iter().take(5), (1, pages)).await;
        let embed = data.into_builder().build();

        MessageBuilder::new().content(content).embed(embed)
    };

    let response_raw = data.create_message(&ctx, builder).await?;

    // Add maps of scores to DB
    let scores_iter = scores.iter().map(|(_, score)| score);

    if let Err(why) = ctx.psql().store_scores_maps(scores_iter).await {
        let report = Report::new(why).wrap_err("error while adding score maps to DB");
        warn!("{:?}", report);
    }

    // Skip pagination if too few entries
    if scores.len() <= 5 {
        return Ok(());
    }

    let response = response_raw.model().await?;

    // Pagination
    let pagination = TopPagination::new(response, user, scores);
    let owner = data.author()?.id;

    tokio::spawn(async move {
        if let Err(why) = pagination.start(&ctx, owner, 60).await {
            let report = Report::new(why).wrap_err("pagination error");
            warn!("{:?}", report);
        }
    });

    Ok(())
}

#[command]
#[short_desc("How many maps of a user's top100 are made by the given mapper?")]
#[long_desc(
    "Display the top plays of a user which were mapped by the given mapper.\n\
    Specify the __user first__ and the __mapper second__.\n\
    Unlike the mapper count of the profile command, this command considers not only \
    the map's creator, but also tries to check if the map is a guest difficulty."
)]
#[usage("[username] [mapper]")]
#[example("badewanne3 \"Hishiro Chizuru\"", "monstrata monstrata")]
pub async fn mapper(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match MapperArgs::args(&ctx, &mut args, msg.author.id, None).await {
                Ok(Ok(mut mapper_args)) => {
                    mapper_args.config.mode.get_or_insert(GameMode::STD);

                    _mapper(ctx, CommandData::Message { msg, args, num }, mapper_args).await
                }
                Ok(Err(content)) => msg.error(&ctx, content).await,
                Err(why) => {
                    let _ = msg.error(&ctx, GENERAL_ISSUE).await;

                    Err(why)
                }
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, *command).await,
    }
}

#[command]
#[short_desc("How many maps of a mania user's top100 are made by the given mapper?")]
#[long_desc(
    "Display the top plays of a mania user which were mapped by the given mapper.\n\
    Specify the __user first__ and the __mapper second__.\n\
    Unlike the mapper count of the profile command, this command considers not only \
    the map's creator, but also tries to check if the map is a guest difficulty.\n\
    If the `-convert` / `-c` argument is specified, I will __not__ count any maps \
    that aren't native mania maps."
)]
#[usage("[username] [mapper] [-convert]")]
#[example("badewanne3 \"Hishiro Chizuru\"", "monstrata monstrata")]
#[aliases("mapperm")]
pub async fn mappermania(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match MapperArgs::args(&ctx, &mut args, msg.author.id, None).await {
                Ok(Ok(mut mapper_args)) => {
                    mapper_args.config.mode = Some(GameMode::MNA);

                    _mapper(ctx, CommandData::Message { msg, args, num }, mapper_args).await
                }
                Ok(Err(content)) => msg.error(&ctx, content).await,
                Err(why) => {
                    let _ = msg.error(&ctx, GENERAL_ISSUE).await;

                    Err(why)
                }
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, *command).await,
    }
}

#[command]
#[short_desc("How many maps of a taiko user's top100 are made by the given mapper?")]
#[long_desc(
    "Display the top plays of a taiko user which were mapped by the given mapper.\n\
    Specify the __user first__ and the __mapper second__.\n\
    Unlike the mapper count of the profile command, this command considers not only \
    the map's creator, but also tries to check if the map is a guest difficulty.\n\
    If the `-convert` / `-c` argument is specified, I will __not__ count any maps \
    that aren't native taiko maps."
)]
#[usage("[username] [mapper] [-convert]")]
#[example("badewanne3 \"Hishiro Chizuru\"", "monstrata monstrata")]
#[aliases("mappert")]
pub async fn mappertaiko(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match MapperArgs::args(&ctx, &mut args, msg.author.id, None).await {
                Ok(Ok(mut mapper_args)) => {
                    mapper_args.config.mode = Some(GameMode::TKO);

                    _mapper(ctx, CommandData::Message { msg, args, num }, mapper_args).await
                }
                Ok(Err(content)) => msg.error(&ctx, content).await,
                Err(why) => {
                    let _ = msg.error(&ctx, GENERAL_ISSUE).await;

                    Err(why)
                }
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, *command).await,
    }
}

#[command]
#[short_desc("How many maps of a ctb user's top100 are made by the given mapper?")]
#[long_desc(
    "Display the top plays of a ctb user which were mapped by the given mapper.\n\
    Specify the __user first__ and the __mapper second__.\n\
    Unlike the mapper count of the profile command, this command considers not only \
    the map's creator, but also tries to check if the map is a guest difficulty.\n\
    If the `-convert` / `-c` argument is specified, I will __not__ count any maps \
    that aren't native ctb maps."
)]
#[usage("[username] [mapper] [-convert]")]
#[example("badewanne3 \"Hishiro Chizuru\"", "monstrata monstrata")]
#[aliases("mapperc")]
async fn mapperctb(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match MapperArgs::args(&ctx, &mut args, msg.author.id, None).await {
                Ok(Ok(mut mapper_args)) => {
                    mapper_args.config.mode = Some(GameMode::CTB);

                    _mapper(ctx, CommandData::Message { msg, args, num }, mapper_args).await
                }
                Ok(Err(content)) => msg.error(&ctx, content).await,
                Err(why) => {
                    let _ = msg.error(&ctx, GENERAL_ISSUE).await;

                    Err(why)
                }
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, *command).await,
    }
}

#[command]
#[short_desc("How many maps of a user's top100 are made by Sotarks?")]
#[long_desc(
    "How many maps of a user's top100 are made by Sotarks?\n\
    Unlike the mapper count of the profile command, this command considers not only \
    the map's creator, but also tries to check if the map is a guest difficulty."
)]
#[usage("[username]")]
#[example("badewanne3")]
pub async fn sotarks(ctx: Arc<Context>, data: CommandData) -> BotResult<()> {
    match data {
        CommandData::Message { msg, mut args, num } => {
            match MapperArgs::args(&ctx, &mut args, msg.author.id, Some("sotarks")).await {
                Ok(Ok(mut mapper_args)) => {
                    mapper_args.config.mode.get_or_insert(GameMode::STD);

                    _mapper(ctx, CommandData::Message { msg, args, num }, mapper_args).await
                }
                Ok(Err(content)) => msg.error(&ctx, content).await,
                Err(why) => {
                    let _ = msg.error(&ctx, GENERAL_ISSUE).await;

                    Err(why)
                }
            }
        }
        CommandData::Interaction { command } => super::slash_top(ctx, *command).await,
    }
}

pub(super) struct MapperArgs {
    config: UserConfig,
    mapper: Name,
}

const TOP_MAPPER: &str = "top mapper";

impl MapperArgs {
    async fn args(
        ctx: &Context,
        args: &mut Args<'_>,
        author_id: UserId,
        mapper: Option<&str>,
    ) -> BotResult<Result<Self, &'static str>> {
        let mut config = ctx.user_config(author_id).await?;

        let (name, mapper) = match args.next() {
            Some(first) => match mapper {
                Some(mapper) => (Some(first), mapper),
                None => match args.next() {
                    Some(second) => (Some(first), second),
                    None => (None, first),
                },
            },
            None => match mapper {
                Some(mapper) => (None, mapper),
                None => {
                    let content = "You need to specify at least one osu username for the mapper. \
                        If you're not linked, you must specify at least two names.";

                    return Ok(Err(content));
                }
            },
        };

        if let Some(name) = name {
            match Args::check_user_mention(ctx, name).await? {
                Ok(osu) => config.osu = Some(osu),
                Err(content) => return Ok(Err(content)),
            }
        }

        let mapper = match Args::check_user_mention(ctx, mapper).await? {
            Ok(osu) => osu.into_username(),
            Err(content) => return Ok(Err(content)),
        };

        Ok(Ok(Self { config, mapper }))
    }

    pub(super) async fn slash(
        ctx: &Context,
        options: Vec<CommandDataOption>,
        author_id: UserId,
    ) -> BotResult<Result<Self, Cow<'static, str>>> {
        let mut config = ctx.user_config(author_id).await?;
        let mut mapper = None;

        for option in options {
            match option {
                CommandDataOption::String { name, value } => match name.as_str() {
                    NAME => config.osu = Some(value.into()),
                    "mapper" => mapper = Some(value.into()),
                    DISCORD => config.osu = Some(parse_discord_option!(ctx, value, "top mapper")),
                    MODE => config.mode = parse_mode_option!(value, "top mapper"),
                    _ => bail_cmd_option!(TOP_MAPPER, string, name),
                },
                CommandDataOption::Integer { name, .. } => {
                    bail_cmd_option!(TOP_MAPPER, integer, name)
                }
                CommandDataOption::Boolean { name, .. } => {
                    bail_cmd_option!(TOP_MAPPER, boolean, name)
                }
                CommandDataOption::SubCommand { name, .. } => {
                    bail_cmd_option!(TOP_MAPPER, subcommand, name)
                }
            }
        }

        let args = Self {
            mapper: mapper.ok_or(Error::InvalidCommandOptions)?,
            config,
        };

        Ok(Ok(args))
    }
}
