use crate::{
    embeds::{osu, Author, EmbedData, Footer},
    util::{
        constants::{AVATAR_URL, OSU_BASE},
        datetime::how_long_ago,
        error::PPError,
        osu::{grade_completion_mods, prepare_beatmap_file},
        ScoreExt,
    },
    BotResult,
};

use rosu::model::{Beatmap, GameMode, Grade, Score, User};
use rosu_pp::{
    Beatmap as Map, BeatmapExt, FruitsPP, GameMode as Mode, ManiaPP, OsuPP, StarResult, TaikoPP,
};
use std::{collections::HashMap, fmt::Write, fs::File};
use twilight_embed_builder::image_source::ImageSource;

pub struct RecentListEmbed {
    description: Option<String>,
    thumbnail: Option<ImageSource>,
    footer: Option<Footer>,
    author: Option<Author>,
    title: &'static str,
}

impl RecentListEmbed {
    pub async fn new<'i, S>(
        user: &User,

        maps: &HashMap<u32, Beatmap>,
        scores: S,
        pages: (usize, usize),
    ) -> BotResult<Self>
    where
        S: Iterator<Item = &'i Score>,
    {
        let idx = (pages.0 - 1) * 10 + 1;

        let mut mod_map = HashMap::new();
        let mut rosu_maps = HashMap::new();

        let mut description = String::with_capacity(512);

        for (i, score) in scores.enumerate() {
            let map = maps.get(&score.beatmap_id.unwrap()).unwrap();

            #[allow(clippy::map_entry)]
            if !rosu_maps.contains_key(&map.beatmap_id) {
                let map_path = prepare_beatmap_file(map.beatmap_id).await?;
                let file = File::open(map_path).map_err(PPError::from)?;
                let rosu_map = Map::parse(file).map_err(PPError::from)?;

                rosu_maps.insert(map.beatmap_id, rosu_map);
            };

            let rosu_map = rosu_maps.get(&map.beatmap_id).unwrap();

            let (pp, stars) = get_pp_stars(&mut mod_map, score, map.beatmap_id, rosu_map);

            let _ = write!(
                description,
                "**{idx}. {grade}\t[{title} [{version}]]({base}b/{id})** [{stars}]",
                idx = idx + i,
                grade = grade_completion_mods(&score, map),
                title = map.title,
                version = map.version,
                base = OSU_BASE,
                id = map.beatmap_id,
                stars = stars,
            );

            if map.mode == GameMode::MNA {
                let _ = write!(description, "\t{}", osu::get_keys(score.enabled_mods, map));
            }

            description.push('\n');

            let _ = writeln!(
                description,
                "{pp}\t[ {combo} ]\t({acc})\t{ago}",
                pp = pp,
                combo = osu::get_combo(score, map),
                acc = score.acc_string(map.mode),
                ago = how_long_ago(&score.date)
            );
        }

        if description.is_empty() {
            description = "No recent scores found".to_owned();
        }

        Ok(Self {
            description: Some(description),
            author: Some(super::get_user_author(user)),
            footer: Some(Footer::new(format!("Page {}/{}", pages.0, pages.1))),
            thumbnail: Some(ImageSource::url(format!("{}{}", AVATAR_URL, user.user_id)).unwrap()),
            title: "List of recent scores:",
        })
    }
}

impl EmbedData for RecentListEmbed {
    fn description_owned(&mut self) -> Option<String> {
        self.description.take()
    }

    fn footer_owned(&mut self) -> Option<Footer> {
        self.footer.take()
    }

    fn author_owned(&mut self) -> Option<Author> {
        self.author.take()
    }

    fn thumbnail_owned(&mut self) -> Option<ImageSource> {
        self.thumbnail.take()
    }

    fn title_owned(&mut self) -> Option<String> {
        Some(self.title.to_owned())
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn footer(&self) -> Option<&Footer> {
        self.footer.as_ref()
    }

    fn author(&self) -> Option<&Author> {
        self.author.as_ref()
    }

    fn thumbnail(&self) -> Option<&ImageSource> {
        self.thumbnail.as_ref()
    }

    fn title(&self) -> Option<&str> {
        Some(self.title)
    }
}

fn get_pp_stars(
    mod_map: &mut HashMap<(u32, u32), (StarResult, f32)>,
    score: &Score,
    map_id: u32,
    map: &Map,
) -> (String, String) {
    let bits = score.enabled_mods.bits();
    let key = (bits, map_id);

    let (mut attributes, mut max_pp) = mod_map.remove(&key).map_or_else(
        || {
            let attributes = map.stars(bits, None);

            (attributes, None)
        },
        |(attributes, max_pp)| (attributes, Some(max_pp)),
    );

    if max_pp.is_none() {
        let result = match map.mode {
            Mode::STD => OsuPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .calculate(),
            Mode::MNA => ManiaPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .calculate(),
            Mode::CTB => FruitsPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .calculate(),
            Mode::TKO => TaikoPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .calculate(),
        };

        max_pp.replace(result.pp());
        attributes = result.attributes;
    }

    let max_pp = max_pp.unwrap();
    let stars = attributes.stars();
    let pp;

    if score.grade == Grade::F {
        let passed = score.total_hits(GameMode::from(map.mode as u8)) as usize;

        let result = match map.mode {
            Mode::STD => OsuPP::new(map)
                .mods(bits)
                .misses(score.count_miss as usize)
                .n300(score.count300 as usize)
                .n100(score.count100 as usize)
                .n50(score.count50 as usize)
                .combo(score.max_combo as usize)
                .passed_objects(passed)
                .calculate(),
            Mode::MNA => ManiaPP::new(map)
                .mods(bits)
                .score(score.score)
                .passed_objects(passed)
                .calculate(),
            Mode::CTB => FruitsPP::new(map)
                .mods(bits)
                .misses(score.count_miss as usize)
                .combo(score.max_combo as usize)
                .fruits(score.count300 as usize)
                .droplets(score.count100 as usize)
                .tiny_droplets(score.count50 as usize)
                .tiny_droplet_misses(score.count_katu as usize)
                .passed_objects(passed - score.count_katu as usize)
                .calculate(),
            Mode::TKO => TaikoPP::new(map)
                .mods(bits)
                .misses(score.count_miss as usize)
                .combo(score.max_combo as usize)
                .n300(score.count300 as usize)
                .n100(score.count100 as usize)
                .passed_objects(passed)
                .calculate(),
        };

        pp = result.pp();
    } else {
        let result = match map.mode {
            Mode::STD => OsuPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .misses(score.count_miss as usize)
                .n300(score.count300 as usize)
                .n100(score.count100 as usize)
                .n50(score.count50 as usize)
                .combo(score.max_combo as usize)
                .calculate(),
            Mode::MNA => ManiaPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .score(score.score)
                .calculate(),
            Mode::CTB => FruitsPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .misses(score.count_miss as usize)
                .combo(score.max_combo as usize)
                .fruits(score.count300 as usize)
                .droplets(score.count100 as usize)
                .tiny_droplets(score.count50 as usize)
                .tiny_droplet_misses(score.count_katu as usize)
                .calculate(),
            Mode::TKO => TaikoPP::new(map)
                .mods(bits)
                .attributes(attributes)
                .misses(score.count_miss as usize)
                .combo(score.max_combo as usize)
                .n300(score.count300 as usize)
                .n100(score.count100 as usize)
                .calculate(),
        };

        pp = result.pp();
        attributes = result.attributes;
    }

    mod_map.insert(key, (attributes, max_pp));

    let pp = format!("**{:.2}**/{:.2}PP", pp, max_pp);
    let stars = osu::get_stars(stars);

    (pp, stars)
}