use std::fmt::{Display, Formatter, Result as FmtResult};

use bathbot_cache::Cache as BathbotCache;
use bathbot_client::Client as BathbotClient;
use bathbot_psql::{model::render::DbRenderOptions, Database};
use eyre::{Result, WrapErr};
use rosu_render::model::{RenderOptions, RenderResolution, RenderSkinOption, Skin, SkinInfo};
use rosu_v2::prelude::{GameMode, Score, ScoreStatistics};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};
use twilight_model::id::{marker::UserMarker, Id};

use crate::core::BotConfig;

#[derive(Copy, Clone)]
pub struct ReplayManager<'d> {
    psql: &'d Database,
    client: &'d BathbotClient,
    cache: &'d BathbotCache,
}

impl<'d> ReplayManager<'d> {
    pub fn new(psql: &'d Database, client: &'d BathbotClient, cache: &'d BathbotCache) -> Self {
        Self {
            psql,
            client,
            cache,
        }
    }

    pub async fn get(
        self,
        score_id: Option<u64>,
        score: &ReplayScore<'_>,
    ) -> Result<Option<Box<[u8]>>> {
        let Some(score_id) = score_id else {
            return Ok(None);
        };

        match self.psql.select_osu_replay(score_id).await {
            Ok(Some(replay)) => return Ok(Some(replay)),
            Ok(None) => {}
            Err(err) => warn!(?err, "Failed to get replay from DB"),
        }

        // If the replay of a score id was not in the database, yet we requested it
        // already, that means the score has no available replay.
        let not_contained = self
            .cache
            .insert_into_set("__requested_replay_score_ids", score_id)
            .await
            .wrap_err("Failed to check whether replay was already requested")?;

        if !not_contained {
            return Ok(None);
        }

        let key = BotConfig::get().tokens.osu_key.as_ref();

        let raw_replay_opt = self
            .client
            .get_raw_osu_replay(key, score_id)
            .await
            .wrap_err("Failed to request replay")?;

        let Some(raw_replay) = raw_replay_opt else {
            return Ok(None);
        };

        let replay = complete_replay(score, score_id, &raw_replay);

        if let Err(err) = self.psql.insert_osu_replay(score_id, &replay).await {
            warn!(?err, "Failed to insert replay into DB");
        }

        Ok(Some(replay))
    }

    pub async fn get_settings(self, user: Id<UserMarker>) -> Result<ReplaySettings> {
        let options = self
            .psql
            .select_user_render_settings(user)
            .await
            .wrap_err("Failed to load settings")?;

        match options {
            Some(options) => Ok(ReplaySettings::from(options)),
            None => {
                let settings = ReplaySettings::default();

                if let Err(err) = self.set_settings(user, &settings).await {
                    warn!(?err);
                }

                Ok(settings)
            }
        }
    }

    pub async fn set_settings<'a>(
        self,
        user: Id<UserMarker>,
        settings: &ReplaySettings,
    ) -> Result<()> {
        let db_options = DbRenderOptions::from(settings);

        self.psql
            .upsert_user_render_settings(user, &db_options)
            .await
            .wrap_err("Failed to upsert settings")
    }

    pub async fn get_video_url(&self, score_id: u64) -> Result<Option<Box<str>>> {
        self.psql
            .select_replay_video_url(score_id)
            .await
            .wrap_err("Failed to get replay video url")
    }

    pub async fn store_video_url(&self, score_id: u64, video_url: &str) -> Result<()> {
        self.psql
            .upsert_replay_video_url(score_id, video_url)
            .await
            .wrap_err("Failed to store replay video url")
    }
}

pub struct ReplaySettings {
    options: RenderOptions,
    skin: RenderSkinOption<'static>,
    skin_presentation_name: Box<str>,
}
impl ReplaySettings {
    pub fn new(
        options: RenderOptions,
        skin: RenderSkinOption<'static>,
        skin_presentation_name: Box<str>,
    ) -> Self {
        Self {
            options,
            skin,
            skin_presentation_name,
        }
    }

    pub fn options(&self) -> &RenderOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut RenderOptions {
        &mut self.options
    }

    pub fn skin(&self) -> &RenderSkinOption<'static> {
        &self.skin
    }

    pub fn official_skin(&mut self, skin: Skin) {
        self.skin = RenderSkinOption::Official {
            name: skin.skin.into_string().into(),
        };
        self.skin_presentation_name = skin.presentation_name;
    }

    pub fn custom_skin(&mut self, id: u32, skin: SkinInfo) {
        self.skin = RenderSkinOption::Custom { id };
        self.skin_presentation_name = skin.name;
    }

    pub fn skin_name(&self) -> SkinName<'_> {
        SkinName {
            name: self.skin_presentation_name.as_ref(),
            custom_skin_id: match self.skin {
                RenderSkinOption::Official { .. } => None,
                RenderSkinOption::Custom { ref id } => Some(*id),
            },
        }
    }
}

pub struct SkinName<'n> {
    name: &'n str,
    custom_skin_id: Option<u32>,
}

impl Display for SkinName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.name)?;

        if let Some(id) = self.custom_skin_id {
            write!(f, " (custom #{id})")?;
        }

        Ok(())
    }
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            options: RenderOptions::default(),
            skin: RenderSkinOption::default(),
            skin_presentation_name: Box::from("Danser default skin (Redd glass)"),
        }
    }
}

impl From<DbRenderOptions> for ReplaySettings {
    fn from(options: DbRenderOptions) -> Self {
        let settings = RenderOptions {
            resolution: RenderResolution::HD720,
            global_volume: options.global_volume as u8,
            music_volume: options.music_volume as u8,
            hitsound_volume: options.hitsound_volume as u8,
            show_hit_error_meter: options.show_hit_error_meter,
            show_unstable_rate: options.show_unstable_rate,
            show_score: options.show_score,
            show_hp_bar: options.show_hp_bar,
            show_combo_counter: options.show_combo_counter,
            show_pp_counter: options.show_pp_counter,
            show_key_overlay: options.show_key_overlay,
            show_scoreboard: options.show_scoreboard,
            show_borders: options.show_borders,
            show_mods: options.show_mods,
            show_result_screen: options.show_result_screen,
            use_skin_cursor: options.use_skin_cursor,
            use_skin_hitsounds: options.use_skin_hitsounds,
            use_beatmap_colors: options.use_beatmap_colors,
            cursor_scale_to_cs: options.cursor_scale_to_cs,
            cursor_rainbow: options.cursor_rainbow,
            cursor_trail_glow: options.cursor_trail_glow,
            draw_follow_points: options.draw_follow_points,
            draw_combo_numbers: options.draw_combo_numbers,
            cursor_size: options.cursor_size,
            cursor_trail: options.cursor_trail,
            beat_scaling: options.beat_scaling,
            slider_merge: options.slider_merge,
            objects_rainbow: options.objects_rainbow,
            flash_objects: options.flash_objects,
            use_slider_hitcircle_color: options.use_slider_hitcircle_color,
            seizure_warning: options.seizure_warning,
            load_storyboard: options.load_storyboard,
            load_video: options.load_video,
            intro_bg_dim: options.intro_bg_dim as u8,
            ingame_bg_dim: options.ingame_bg_dim as u8,
            break_bg_dim: options.break_bg_dim as u8,
            bg_parallax: options.bg_parallax,
            show_danser_logo: options.show_danser_logo,
            skip_intro: options.skip_intro,
            cursor_ripples: options.cursor_ripples,
            slider_snaking_in: options.slider_snaking_in,
            slider_snaking_out: options.slider_snaking_out,
            show_hit_counter: options.show_hit_counter,
            show_avatars_on_scoreboard: options.show_avatars_on_scoreboard,
            show_aim_error_meter: options.show_aim_error_meter,
            play_nightcore_samples: options.play_nightcore_samples,
        };

        let skin = match (options.skin_id, options.skin_name) {
            (None, Some(name)) => RenderSkinOption::Official { name: name.into() },
            (Some(id), None) => RenderSkinOption::Custom { id: id as u32 },
            (Some(_), Some(_)) | (None, None) => unreachable!(),
        };

        Self {
            options: settings,
            skin,
            skin_presentation_name: options.skin_presentation_name.into_boxed_str(),
        }
    }
}

impl From<&ReplaySettings> for DbRenderOptions {
    fn from(settings: &ReplaySettings) -> Self {
        let ReplaySettings {
            options,
            skin,
            skin_presentation_name,
        } = settings;

        let (skin_id, skin_name) = match skin {
            RenderSkinOption::Official { name } => (None, Some(name.as_ref().to_owned())),
            RenderSkinOption::Custom { id } => (Some(*id as i32), None),
        };

        Self {
            skin_id,
            skin_name,
            skin_presentation_name: skin_presentation_name.as_ref().to_owned(),
            global_volume: options.global_volume as i16,
            music_volume: options.music_volume as i16,
            hitsound_volume: options.hitsound_volume as i16,
            show_hit_error_meter: options.show_hit_error_meter,
            show_unstable_rate: options.show_unstable_rate,
            show_score: options.show_score,
            show_hp_bar: options.show_hp_bar,
            show_combo_counter: options.show_combo_counter,
            show_pp_counter: options.show_pp_counter,
            show_key_overlay: options.show_key_overlay,
            show_scoreboard: options.show_scoreboard,
            show_borders: options.show_borders,
            show_mods: options.show_mods,
            show_result_screen: options.show_result_screen,
            use_skin_cursor: options.use_skin_cursor,
            use_skin_hitsounds: options.use_skin_hitsounds,
            use_beatmap_colors: options.use_beatmap_colors,
            cursor_scale_to_cs: options.cursor_scale_to_cs,
            cursor_rainbow: options.cursor_rainbow,
            cursor_trail_glow: options.cursor_trail_glow,
            draw_follow_points: options.draw_follow_points,
            draw_combo_numbers: options.draw_combo_numbers,
            cursor_size: options.cursor_size,
            cursor_trail: options.cursor_trail,
            beat_scaling: options.beat_scaling,
            slider_merge: options.slider_merge,
            objects_rainbow: options.objects_rainbow,
            flash_objects: options.flash_objects,
            use_slider_hitcircle_color: options.use_slider_hitcircle_color,
            seizure_warning: options.seizure_warning,
            load_storyboard: options.load_storyboard,
            load_video: options.load_video,
            intro_bg_dim: options.intro_bg_dim as i16,
            ingame_bg_dim: options.ingame_bg_dim as i16,
            break_bg_dim: options.break_bg_dim as i16,
            bg_parallax: options.bg_parallax,
            show_danser_logo: options.show_danser_logo,
            skip_intro: options.skip_intro,
            cursor_ripples: options.cursor_ripples,
            slider_snaking_in: options.slider_snaking_in,
            slider_snaking_out: options.slider_snaking_out,
            show_hit_counter: options.show_hit_counter,
            show_avatars_on_scoreboard: options.show_avatars_on_scoreboard,
            show_aim_error_meter: options.show_aim_error_meter,
            play_nightcore_samples: options.play_nightcore_samples,
        }
    }
}
pub enum ReplayScore<'s> {
    Owned(OwnedReplayScore),
    Borrowed(&'s Score),
}

impl<'s> From<&'s Score> for ReplayScore<'s> {
    fn from(score: &'s Score) -> Self {
        Self::Borrowed(score)
    }
}

impl ReplayScore<'_> {
    fn mode(&self) -> GameMode {
        match self {
            Self::Owned(score) => score.mode,
            Self::Borrowed(score) => score.mode,
        }
    }

    fn ended_at(&self) -> OffsetDateTime {
        match self {
            Self::Owned(score) => score.ended_at,
            Self::Borrowed(score) => score.ended_at,
        }
    }

    fn map_checksum(&self) -> Option<&str> {
        match self {
            Self::Owned(score) => score.map_checksum.as_deref(),
            Self::Borrowed(score) => score.map.as_ref().and_then(|map| map.checksum.as_deref()),
        }
    }

    fn username(&self) -> &str {
        match self {
            Self::Owned(score) => score.username.as_ref(),
            Self::Borrowed(score) => score
                .user
                .as_ref()
                .map(|user| user.username.as_str())
                .unwrap_or_default(),
        }
    }

    fn statistics(&self) -> &ScoreStatistics {
        match self {
            Self::Owned(score) => &score.statistics,
            Self::Borrowed(score) => &score.statistics,
        }
    }

    fn score(&self) -> u32 {
        match self {
            Self::Owned(score) => score.score,
            Self::Borrowed(score) => score.score,
        }
    }

    fn max_combo(&self) -> u16 {
        match self {
            Self::Owned(score) => score.max_combo,
            Self::Borrowed(score) => score.max_combo as u16,
        }
    }

    fn perfect(&self) -> bool {
        match self {
            Self::Owned(score) => score.perfect,
            Self::Borrowed(score) => score.perfect,
        }
    }

    fn mods(&self) -> u32 {
        match self {
            Self::Owned(score) => score.mods,
            Self::Borrowed(score) => score.mods.bits(),
        }
    }
}

pub struct OwnedReplayScore {
    mode: GameMode,
    ended_at: OffsetDateTime,
    map_checksum: Option<Box<str>>,
    username: Box<str>,
    statistics: ScoreStatistics,
    score: u32,
    max_combo: u16,
    perfect: bool,
    mods: u32,
}

impl From<&Score> for OwnedReplayScore {
    fn from(score: &Score) -> Self {
        Self {
            mode: score.mode,
            ended_at: score.ended_at,
            map_checksum: score
                .map
                .as_ref()
                .and_then(|map| map.checksum.as_deref())
                .map(Box::from),
            username: score
                .user
                .as_ref()
                .map(|user| user.username.as_str())
                .unwrap_or_default()
                .into(),
            statistics: score.statistics.clone(),
            score: score.score,
            max_combo: score.max_combo as u16,
            perfect: score.perfect,
            mods: score.mods.bits(),
        }
    }
}

// https://osu.ppy.sh/wiki/en/Client/File_formats/Osr_%28file_format%29
fn complete_replay(score: &ReplayScore<'_>, score_id: u64, raw_replay: &[u8]) -> Box<[u8]> {
    let mut replay = Vec::with_capacity(128 + raw_replay.len());

    let mut bytes_written = 0;

    bytes_written += encode_byte(&mut replay, score.mode() as u8);
    bytes_written += encode_int(&mut replay, game_version(score.ended_at().date()));

    let map_md5 = score.map_checksum().unwrap_or_default();
    bytes_written += encode_string(&mut replay, map_md5);

    let username = score.username();
    bytes_written += encode_string(&mut replay, username);

    let replay_md5 = String::new();
    bytes_written += encode_string(&mut replay, &replay_md5);

    let stats = score.statistics();
    bytes_written += encode_short(&mut replay, stats.count_300 as u16);
    bytes_written += encode_short(&mut replay, stats.count_100 as u16);
    bytes_written += encode_short(&mut replay, stats.count_50 as u16);
    bytes_written += encode_short(&mut replay, stats.count_geki as u16);
    bytes_written += encode_short(&mut replay, stats.count_katu as u16);
    bytes_written += encode_short(&mut replay, stats.count_miss as u16);

    bytes_written += encode_int(&mut replay, score.score());

    bytes_written += encode_short(&mut replay, score.max_combo());

    bytes_written += encode_byte(&mut replay, score.perfect() as u8);

    bytes_written += encode_int(&mut replay, score.mods());

    let lifebar = String::new();
    bytes_written += encode_string(&mut replay, &lifebar);

    bytes_written += encode_datetime(&mut replay, score.ended_at());

    bytes_written += encode_int(&mut replay, raw_replay.len() as u32);
    replay.extend_from_slice(raw_replay);

    bytes_written += encode_long(&mut replay, score_id);

    if bytes_written > 128 {
        warn!(bytes_written, "Wrote more bytes than initial allocation");
    }

    replay.into_boxed_slice()
}

fn encode_byte(bytes: &mut Vec<u8>, byte: u8) -> usize {
    bytes.push(byte);

    1
}

fn encode_short(bytes: &mut Vec<u8>, short: u16) -> usize {
    bytes.extend_from_slice(&short.to_le_bytes());

    2
}

fn encode_int(bytes: &mut Vec<u8>, int: u32) -> usize {
    bytes.extend_from_slice(&int.to_le_bytes());

    4
}

fn encode_long(bytes: &mut Vec<u8>, long: u64) -> usize {
    bytes.extend_from_slice(&long.to_le_bytes());

    8
}

fn encode_string(bytes: &mut Vec<u8>, s: &str) -> usize {
    if s.is_empty() {
        bytes.push(0x00); // "no string"

        1
    } else {
        bytes.push(0x0b); // "string incoming"
        let len = encode_leb128(bytes, s.len());
        bytes.extend_from_slice(s.as_bytes());

        1 + len + s.len()
    }
}

// https://en.wikipedia.org/wiki/LEB128
fn encode_leb128(bytes: &mut Vec<u8>, mut n: usize) -> usize {
    let mut bytes_written = 0;

    loop {
        let mut byte = ((n & u8::MAX as usize) as u8) & !(1 << 7);
        n >>= 7;

        if n != 0 {
            byte |= 1 << 7;
        }

        bytes.push(byte);
        bytes_written += 1;

        if n == 0 {
            return bytes_written;
        }
    }
}

// https://docs.microsoft.com/en-us/dotnet/api/system.datetime.ticks?redirectedfrom=MSDN&view=net-6.0#System_DateTime_Ticks
fn encode_datetime(bytes: &mut Vec<u8>, datetime: OffsetDateTime) -> usize {
    let orig_date = Date::from_ordinal_date(1, 1).unwrap();
    let orig_time = Time::from_hms(0, 0, 0).unwrap();

    let orig = PrimitiveDateTime::new(orig_date, orig_time).assume_utc();

    let orig_nanos = orig.unix_timestamp_nanos();
    let this_nanos = datetime.unix_timestamp_nanos();

    let long = (this_nanos - orig_nanos) / 100;

    encode_long(bytes, long as u64)
}

fn game_version(date: Date) -> u32 {
    let mut version = date.year() as u32;
    version *= 100;

    version += date.month() as u32;
    version *= 100;

    version += date.day() as u32;

    version
}