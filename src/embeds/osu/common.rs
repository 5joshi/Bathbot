use crate::{
    commands::osu::{CommonScoreEntry, CommonUser},
    embeds::attachment,
    util::{builder::FooterBuilder, constants::OSU_BASE},
};

use smallvec::SmallVec;
use std::fmt::Write;

pub struct CommonEmbed {
    description: String,
    thumbnail: String,
    footer: FooterBuilder,
}

type CommonScore = SmallVec<[CommonScoreEntry; 3]>;

impl CommonEmbed {
    pub fn new(users: &[CommonUser], scores: &[CommonScore], index: usize) -> Self {
        let mut description = String::with_capacity(512);

        for (i, scores) in scores.iter().enumerate() {
            let (title, version, map_id) = {
                let first = scores.first().unwrap();
                let map = first.score.map.as_ref().unwrap();

                (
                    &first.score.mapset.as_ref().unwrap().title,
                    &map.version,
                    map.map_id,
                )
            };

            let _ = writeln!(
                description,
                "**{idx}.** [{title} [{version}]]({base}b/{id})",
                idx = index + i + 1,
                title = title,
                version = version,
                base = OSU_BASE,
                id = map_id,
            );

            description.push('-');

            for CommonScoreEntry { pos, pp, score } in scores.iter() {
                let _ = write!(
                    description,
                    " :{medal}_place: `{name}`: {pp:.2}pp",
                    medal = match pos {
                        0 => "first",
                        1 => "second",
                        2 => "third",
                        _ => unreachable!(),
                    },
                    name = score.user.as_ref().unwrap().username,
                    pp = pp,
                );
            }

            description.push('\n');
        }

        description.pop();

        let mut footer = String::with_capacity(64);
        footer.push_str("🥇 count");

        for user in users {
            let _ = write!(footer, " | {}: {}", user.name(), user.first_count);
        }

        Self {
            footer: FooterBuilder::new(footer),
            description,
            thumbnail: attachment("avatar_fuse.png"),
        }
    }
}

impl_builder!(CommonEmbed {
    description,
    footer,
    thumbnail
});
