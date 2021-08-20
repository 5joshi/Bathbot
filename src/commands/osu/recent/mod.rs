mod leaderboard;
mod list;
mod pages;
mod recent;
mod simulate;

pub use leaderboard::*;
pub use list::*;
pub use pages::*;
pub use recent::*;
pub use simulate::*;

use super::{prepare_score, prepare_scores, request_user, require_link, ErrorType, GradeArg};
use crate::{
    util::{osu::ModSelection, ApplicationCommandExt, MessageExt},
    BotResult, Context, Error,
};

use rosu_v2::prelude::GameMode;
use std::{borrow::Cow, sync::Arc};
use twilight_model::application::{
    command::{
        BaseCommandOptionData, ChoiceCommandOptionData, Command, CommandOption,
        OptionsCommandOptionData,
    },
    interaction::{application_command::CommandDataOption, ApplicationCommand},
};

enum RecentCommandKind {
    Leaderboard(RecentLeaderboardArgs),
    List(RecentListArgs),
    Score(RecentArgs),
    Simulate(RecentSimulateArgs),
}

impl RecentCommandKind {
    fn slash(
        ctx: &Context,
        command: &mut ApplicationCommand,
    ) -> BotResult<Result<Self, Cow<'static, str>>> {
        let mut kind = None;

        for option in command.yoink_options() {
            match option {
                CommandDataOption::String { name, .. } => bail_cmd_option!("recent", string, name),
                CommandDataOption::Integer { name, .. } => {
                    bail_cmd_option!("recent", integer, name)
                }
                CommandDataOption::Boolean { name, .. } => {
                    bail_cmd_option!("recent", boolean, name)
                }
                CommandDataOption::SubCommand { name, options } => match name.as_str() {
                    "score" => {
                        let mut username = None;
                        let mut mode = None;
                        let mut index = None;

                        for option in options {
                            match option {
                                CommandDataOption::String { name, value } => match name.as_str() {
                                    "name" => username = Some(value.into()),
                                    "discord" => match value.parse() {
                                        Ok(id) => match ctx.get_link(id) {
                                            Some(name) => username = Some(name),
                                            None => {
                                                let content = format!(
                                                    "<@{}> is not linked to an osu profile",
                                                    id
                                                );

                                                return Ok(Err(content.into()));
                                            }
                                        },
                                        Err(_) => {
                                            bail_cmd_option!("recent score discord", string, value)
                                        }
                                    },
                                    "mode" => mode = parse_mode_option!(value, "recent score"),
                                    _ => bail_cmd_option!("recent score", string, name),
                                },
                                CommandDataOption::Integer { name, value } => match name.as_str() {
                                    "index" => index = Some(value.max(1).min(50) as usize),
                                    _ => bail_cmd_option!("recent score", integer, name),
                                },
                                CommandDataOption::Boolean { name, .. } => {
                                    bail_cmd_option!("recent score", boolean, name)
                                }
                                CommandDataOption::SubCommand { name, .. } => {
                                    bail_cmd_option!("recent score", subcommand, name)
                                }
                            }
                        }

                        let args = RecentArgs {
                            name: username,
                            mode: mode.unwrap_or(GameMode::STD),
                            index,
                        };

                        kind = Some(RecentCommandKind::Score(args));
                    }
                    "leaderboard" => {
                        let mut username = None;
                        let mut mode = None;
                        let mut mods = None;
                        let mut index = None;

                        for option in options {
                            match option {
                                CommandDataOption::String { name, value } => match name.as_str() {
                                    "name" => username = Some(value.into()),
                                    "mods" => match value.parse() {
                                        Ok(m) => mods = Some(ModSelection::Include(m)),
                                        Err(_) => {
                                            let content = "Failed to parse mods. Be sure to specify a valid abbreviation e.g. hdhr.";

                                            return Ok(Err(content.into()));
                                        }
                                    },
                                    "discord" => match value.parse() {
                                        Ok(id) => match ctx.get_link(id) {
                                            Some(name) => username = Some(name),
                                            None => {
                                                let content = format!(
                                                    "<@{}> is not linked to an osu profile",
                                                    id
                                                );

                                                return Ok(Err(content.into()));
                                            }
                                        },
                                        Err(_) => {
                                            bail_cmd_option!(
                                                "recent leaderboard discord",
                                                string,
                                                value
                                            )
                                        }
                                    },
                                    "mode" => {
                                        mode = parse_mode_option!(value, "recent leaderboard")
                                    }
                                    _ => bail_cmd_option!("recent leaderboard", string, name),
                                },
                                CommandDataOption::Integer { name, value } => match name.as_str() {
                                    "index" => index = Some(value.max(1).min(50) as usize),
                                    _ => bail_cmd_option!("recent leaderboard", integer, name),
                                },
                                CommandDataOption::Boolean { name, .. } => {
                                    bail_cmd_option!("recent leaderboard", boolean, name)
                                }
                                CommandDataOption::SubCommand { name, .. } => {
                                    bail_cmd_option!("recent leaderboard", subcommand, name)
                                }
                            }
                        }

                        let args = RecentLeaderboardArgs {
                            name: username,
                            mode: mode.unwrap_or(GameMode::STD),
                            mods,
                            index,
                        };

                        kind = Some(RecentCommandKind::Leaderboard(args));
                    }
                    "list" => {
                        let mut username = None;
                        let mut mode = None;

                        for option in options {
                            match option {
                                CommandDataOption::String { name, value } => match name.as_str() {
                                    "name" => username = Some(value.into()),
                                    "discord" => match value.parse() {
                                        Ok(id) => match ctx.get_link(id) {
                                            Some(name) => username = Some(name),
                                            None => {
                                                let content = format!(
                                                    "<@{}> is not linked to an osu profile",
                                                    id
                                                );

                                                return Ok(Err(content.into()));
                                            }
                                        },
                                        Err(_) => {
                                            bail_cmd_option!("recent list discord", string, value)
                                        }
                                    },
                                    "mode" => mode = parse_mode_option!(value, "recent list"),
                                    _ => bail_cmd_option!("recent list", string, name),
                                },
                                CommandDataOption::Integer { name, .. } => {
                                    bail_cmd_option!("recent list", integer, name)
                                }
                                CommandDataOption::Boolean { name, .. } => {
                                    bail_cmd_option!("recent list", boolean, name)
                                }
                                CommandDataOption::SubCommand { name, .. } => {
                                    bail_cmd_option!("recent list", subcommand, name)
                                }
                            }
                        }

                        let args = RecentListArgs {
                            name: username,
                            mode: mode.unwrap_or(GameMode::STD),
                        };

                        kind = Some(RecentCommandKind::List(args));
                    }
                    "simulate" => {
                        let mut username = None;
                        let mut mode = None;
                        let mut mods = None;
                        let mut index = None;
                        let mut n300 = None;
                        let mut n100 = None;
                        let mut n50 = None;
                        let mut misses = None;
                        let mut acc = None;
                        let mut combo = None;
                        let mut score = None;

                        for option in options {
                            match option {
                                CommandDataOption::String { name, value } => match name.as_str() {
                                    "name" => username = Some(value.into()),
                                    "mods" => match value.parse() {
                                        Ok(m) => mods = Some(ModSelection::Include(m)),
                                        Err(_) => {
                                            let content = "Failed to parse mods. Be sure to specify a valid abbreviation e.g. `hdhr`.";

                                            return Ok(Err(content.into()));
                                        }
                                    },
                                    "discord" => match value.parse() {
                                        Ok(id) => match ctx.get_link(id) {
                                            Some(name) => username = Some(name),
                                            None => {
                                                let content = format!(
                                                    "<@{}> is not linked to an osu profile",
                                                    id
                                                );

                                                return Ok(Err(content.into()));
                                            }
                                        },
                                        Err(_) => {
                                            bail_cmd_option!(
                                                "recent simulate discord",
                                                string,
                                                value
                                            )
                                        }
                                    },
                                    "mode" => mode = parse_mode_option!(value, "recent simulate"),
                                    _ => bail_cmd_option!("recent simulate", string, name),
                                },
                                CommandDataOption::Integer { name, value } => match name.as_str() {
                                    "index" => index = Some(value.max(1).min(50) as usize),
                                    "n300" => n300 = Some(value.max(0) as usize),
                                    "n100" => n100 = Some(value.max(0) as usize),
                                    "n50" => n50 = Some(value.max(0) as usize),
                                    "misses" => misses = Some(value.max(0) as usize),
                                    "combo" => combo = Some(value.max(0) as usize),
                                    "score" => score = Some(value.max(0) as u32),
                                    _ => bail_cmd_option!("recent simulate", integer, name),
                                },
                                CommandDataOption::Boolean { name, .. } => {
                                    bail_cmd_option!("recent simulate", boolean, name)
                                }
                                CommandDataOption::SubCommand { name, .. } => {
                                    bail_cmd_option!("recent simulate", subcommand, name)
                                }
                            }
                        }

                        let args = RecentSimulateArgs {
                            name: username,
                            mode: mode.unwrap_or(GameMode::STD),
                            mods,
                            index,
                            n300,
                            n100,
                            n50,
                            misses,
                            acc,
                            combo,
                            score,
                        };

                        kind = Some(RecentCommandKind::Simulate(args));
                    }
                    _ => bail_cmd_option!("recent", subcommand, name),
                },
            }
        }

        kind.ok_or(Error::InvalidCommandOptions).map(Ok)
    }
}

pub async fn slash_recent(ctx: Arc<Context>, mut command: ApplicationCommand) -> BotResult<()> {
    match RecentCommandKind::slash(&ctx, &mut command)? {
        Ok(RecentCommandKind::Score(args)) => _recent(ctx, command.into(), args).await,
        Ok(RecentCommandKind::Leaderboard(args)) => {
            _recentleaderboard(ctx, command.into(), args, false).await
        }
        Ok(RecentCommandKind::List(args)) => _recentlist(ctx, command.into(), args).await,
        Ok(RecentCommandKind::Simulate(args)) => _recentsimulate(ctx, command.into(), args).await,
        Err(msg) => command.error(&ctx, msg).await,
    }
}

pub fn slash_recent_command() -> Command {
    Command {
        application_id: None,
        guild_id: None,
        name: "recent".to_owned(),
        default_permission: None,
        description: "Display info about a user's recent play".to_owned(),
        id: None,
        options: vec![
            CommandOption::SubCommand(OptionsCommandOptionData {
                description: "Show a user's recent score".to_owned(),
                name: "score".to_owned(),
                options: vec![
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: super::mode_choices(),
                        description: "Specify the gamemode".to_owned(),
                        name: "mode".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify a username".to_owned(),
                        name: "name".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Choose the recent score's index e.g. 1 for most recent"
                            .to_owned(),
                        name: "index".to_owned(),
                        required: false,
                    }),
                    CommandOption::User(BaseCommandOptionData {
                        description: "Specify a linked discord user".to_owned(),
                        name: "discord".to_owned(),
                        required: false,
                    }),
                ],
                required: false,
            }),
            CommandOption::SubCommand(OptionsCommandOptionData {
                description: "Show the leaderboard of a user's recently played map".to_owned(),
                name: "leaderboard".to_owned(),
                options: vec![
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: super::mode_choices(),
                        description: "Specify the gamemode".to_owned(),
                        name: "mode".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify a username".to_owned(),
                        name: "name".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify mods e.g. hdhr or nm".to_owned(),
                        name: "mods".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Choose the recent score's index e.g. 1 for most recent"
                            .to_owned(),
                        name: "index".to_owned(),
                        required: false,
                    }),
                    CommandOption::User(BaseCommandOptionData {
                        description: "Specify a linked discord user".to_owned(),
                        name: "discord".to_owned(),
                        required: false,
                    }),
                ],
                required: false,
            }),
            CommandOption::SubCommand(OptionsCommandOptionData {
                description: "Show all recent plays of a user".to_owned(),
                name: "list".to_owned(),
                options: vec![
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: super::mode_choices(),
                        description: "Specify the gamemode".to_owned(),
                        name: "mode".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify a username".to_owned(),
                        name: "name".to_owned(),
                        required: false,
                    }),
                    CommandOption::User(BaseCommandOptionData {
                        description: "Specify a linked discord user".to_owned(),
                        name: "discord".to_owned(),
                        required: false,
                    }),
                ],
                required: false,
            }),
            CommandOption::SubCommand(OptionsCommandOptionData {
                description: "Unchoke a user's recent score or simulate a play on its map"
                    .to_owned(),
                name: "simulate".to_owned(),
                options: vec![
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: super::mode_choices(),
                        description: "Specify the gamemode".to_owned(),
                        name: "mode".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify a username".to_owned(),
                        name: "name".to_owned(),
                        required: false,
                    }),
                    CommandOption::String(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify mods e.g. hdhr or nm".to_owned(),
                        name: "mods".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Choose the recent score's index e.g. 1 for most recent"
                            .to_owned(),
                        name: "index".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the amount of 300s".to_owned(),
                        name: "n300".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the amount of 100s".to_owned(),
                        name: "n100".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the amount of 50s".to_owned(),
                        name: "n50".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the amount of misses".to_owned(),
                        name: "misses".to_owned(),
                        required: false,
                    }),
                    // TODO
                    // CommandOption::Number(ChoiceCommandOptionData {
                    //     choices: vec![],
                    //     description: "Specify the accuracy".to_owned(),
                    //     name: "acc".to_owned(),
                    //     required: false,
                    // }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the combo".to_owned(),
                        name: "combo".to_owned(),
                        required: false,
                    }),
                    CommandOption::Integer(ChoiceCommandOptionData {
                        choices: vec![],
                        description: "Specify the score (only relevant for mania)".to_owned(),
                        name: "score".to_owned(),
                        required: false,
                    }),
                    CommandOption::User(BaseCommandOptionData {
                        description: "Specify a linked discord user".to_owned(),
                        name: "discord".to_owned(),
                        required: false,
                    }),
                ],
                required: false,
            }),
        ],
    }
}