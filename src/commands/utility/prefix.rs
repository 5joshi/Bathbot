use crate::{
    database::Prefix,
    util::{matcher, MessageExt},
    Args, BotResult, Context,
};

use std::{cmp::Ordering, fmt::Write, sync::Arc};
use twilight_model::channel::Message;

#[command]
#[short_desc("Change my prefixes for a server")]
#[long_desc(
    "Change my prefixes for a server.\n\
    To check the current prefixes for this server, \
    don't pass any arguments.\n\
    Otherwise, the first argument must be either `add` or `remove`.\n\
    Following that must be a space-separated list of \
    characters or strings you want to add or remove as prefix.\n\
    Guilds must have between one and five prefixes."
)]
#[only_guilds()]
#[authority()]
#[usage("[add / remove] [prefix]")]
#[example("add $ 🍆 new_pref", "remove < !!")]
#[aliases("prefixes")]
async fn prefix(ctx: Arc<Context>, msg: &Message, mut args: Args) -> BotResult<()> {
    let guild_id = msg.guild_id.unwrap();

    let action = match args.next() {
        Some("add") | Some("a") => Action::Add,
        Some("remove") | Some("r") => Action::Remove,
        Some(other) => {
            let content = format!(
                "If any arguments are provided, the first one \
                must be either `add` or `remove`, not `{}`",
                other
            );

            return msg.error(&ctx, content).await;
        }
        None => {
            let prefixes = ctx.config_prefixes(guild_id);
            let mut content = String::new();
            current_prefixes(&mut content, &prefixes);
            msg.send_response(&ctx, content).await?;

            return Ok(());
        }
    };

    if args.is_empty() {
        let content = "After the first argument you should specify some prefix(es)";

        return msg.error(&ctx, content).await;
    }

    let args: Vec<Prefix> = args.take(5).map(|arg| arg.into()).collect();

    if args.iter().any(|arg| matcher::is_custom_emote(arg)) {
        let content = "Does not work with custom emotes unfortunately \\:(";

        return msg.error(&ctx, content).await;
    }

    ctx.update_config(guild_id, |config| match action {
        Action::Add => {
            config.prefixes.extend(args);
            config.prefixes.sort_unstable_by(|a, b| {
                if a == "<" {
                    Ordering::Less
                } else if b == "<" {
                    Ordering::Greater
                } else {
                    a.cmp(&b)
                }
            });
            config.prefixes.dedup();
            config.prefixes.truncate(5);
        }
        Action::Remove => {
            for arg in args {
                config.prefixes.retain(|p| p != &arg);
                if config.prefixes.is_empty() {
                    config.prefixes.push(arg);

                    break;
                }
            }
        }
    });

    let mut content = String::from("Prefixes updated!\n");
    let prefixes = ctx.config_prefixes(guild_id);
    current_prefixes(&mut content, &prefixes);
    msg.send_response(&ctx, content).await?;

    Ok(())
}

enum Action {
    Add,
    Remove,
}

fn current_prefixes(content: &mut String, prefixes: &[Prefix]) {
    content.push_str("Prefixes for this server: ");
    let len = prefixes.iter().map(|p| p.len() + 4).sum();
    content.reserve_exact(len);
    let mut prefixes = prefixes.iter();

    if let Some(first) = prefixes.next() {
        let _ = write!(content, "`{}`", first);

        for prefix in prefixes {
            let _ = write!(content, ", `{}`", prefix);
        }
    }
}
