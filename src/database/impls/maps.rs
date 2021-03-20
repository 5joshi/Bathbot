use crate::{
    database::{DBBeatmap, DBBeatmapset},
    BotResult, Database,
};

use chrono::{DateTime, Utc};
use futures::{
    future::{BoxFuture, FutureExt},
    stream::{StreamExt, TryStreamExt},
};
use rosu_v2::prelude::{
    Beatmap, Beatmapset, BeatmapsetCompact,
    RankStatus::{Approved, Loved, Ranked},
    Score,
};
use sqlx::PgConnection;
use std::collections::HashMap;

macro_rules! invalid_status {
    ($obj:ident) => {
        !matches!($obj.status, Ranked | Loved | Approved)
    };
}

impl Database {
    pub async fn get_beatmap(&self, map_id: u32, with_mapset: bool) -> BotResult<Beatmap> {
        let mut conn = self.pool.acquire().await?;

        let map = sqlx::query_as!(
            DBBeatmap,
            "SELECT * FROM maps WHERE map_id=$1",
            map_id as i32
        )
        .fetch_one(&mut conn)
        .await?;

        let mut map: Beatmap = map.into();

        if with_mapset {
            let mapset = sqlx::query_as!(
                DBBeatmapset,
                "SELECT * FROM mapsets WHERE mapset_id=$1",
                map.mapset_id as i32
            )
            .fetch_one(&mut conn)
            .await?;

            map.mapset.replace(mapset.into());
        }

        Ok(map)
    }

    pub async fn get_beatmapset<T: From<DBBeatmapset>>(&self, mapset_id: u32) -> BotResult<T> {
        let mapset = sqlx::query_as!(
            DBBeatmapset,
            "SELECT * FROM mapsets WHERE mapset_id=$1",
            mapset_id as i32
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(mapset.into())
    }

    pub async fn get_beatmap_combo(&self, map_id: u32) -> BotResult<Option<u32>> {
        let row = sqlx::query!("SELECT max_combo FROM maps WHERE map_id=$1", map_id as i32)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.max_combo.map(|c| c as u32))
    }

    pub async fn get_beatmaps_combo(
        &self,
        map_ids: &[i32],
    ) -> BotResult<HashMap<u32, Option<u32>>> {
        let mut combos = HashMap::with_capacity(map_ids.len());

        let mut rows = sqlx::query!(
            "SELECT map_id,max_combo FROM maps WHERE map_id=ANY($1)",
            map_ids
        )
        .fetch(&self.pool);

        while let Some(row) = rows.next().await.transpose()? {
            combos.insert(row.map_id as u32, row.max_combo.map(|c| c as u32));
        }

        Ok(combos)
    }

    pub async fn get_beatmaps(
        &self,
        map_ids: &[i32],
        with_mapset: bool,
    ) -> BotResult<HashMap<u32, Beatmap>> {
        if map_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.pool.acquire().await?;

        let mut stream = sqlx::query_as!(
            DBBeatmap,
            "SELECT * FROM maps WHERE map_id=ANY($1)",
            map_ids
        )
        .fetch(&mut conn)
        .map_ok(Beatmap::from)
        .map_ok(|m| (m.map_id, m));

        let mut beatmaps = HashMap::with_capacity(map_ids.len());

        while let Some((id, mut map)) = stream.next().await.transpose()? {
            if with_mapset {
                let mapset = sqlx::query_as!(
                    DBBeatmapset,
                    "SELECT * FROM mapsets WHERE mapset_id=$1",
                    map.mapset_id as i32
                )
                .fetch_one(&self.pool)
                .await?;

                map.mapset.replace(mapset.into());
            }

            beatmaps.insert(id, map);
        }

        Ok(beatmaps)
    }

    pub async fn insert_beatmapset(&self, mapset: &Beatmapset) -> BotResult<bool> {
        if invalid_status!(mapset) {
            return Ok(false);
        }

        let mut conn = self.pool.acquire().await?;

        _insert_mapset(&mut conn, mapset).await.map(|_| true)
    }

    pub async fn insert_beatmap(&self, map: &Beatmap) -> BotResult<bool> {
        if invalid_status!(map) {
            return Ok(false);
        }

        let mut conn = self.pool.acquire().await?;

        _insert_map(&mut conn, map).await.map(|_| true)
    }

    pub async fn insert_beatmaps(&self, maps: &[Beatmap]) -> BotResult<usize> {
        let mut conn = self.pool.acquire().await?;

        let mut count = 0;

        for map in maps {
            if invalid_status!(map) {
                continue;
            }

            _insert_map(&mut conn, map).await?;

            count += 1;
        }

        Ok(count)
    }

    pub async fn store_scores_maps<'s>(
        &self,
        scores: impl Iterator<Item = &'s Score>,
    ) -> BotResult<(usize, usize)> {
        let mut conn = self.pool.acquire().await?;

        let mut maps = 0;
        let mut mapsets = 0;

        for score in scores {
            if let Some(ref map) = score.map {
                if invalid_status!(map) {
                    continue;
                }

                _insert_map(&mut conn, map).await?;

                maps += 1;

                if let Some(ref mapset) = score.mapset {
                    if invalid_status!(mapset) {
                        continue;
                    }

                    _insert_mapset_compact(&mut conn, mapset, map.last_updated).await?;

                    mapsets += 1;
                }
            }
        }

        Ok((maps, mapsets))
    }
}

async fn _insert_map(conn: &mut PgConnection, map: &Beatmap) -> BotResult<()> {
    sqlx::query!(
        "INSERT INTO maps (map_id,mapset_id,user_id,checksum,version,seconds_total,
        seconds_drain,count_circles,count_sliders,count_spinners,hp,cs,od,ar,mode,
        status,last_update,stars,bpm) VALUES
        ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
        ON CONFLICT (map_id) DO NOTHING",
        map.map_id as i32,
        map.mapset_id as i32,
        map.mapset.as_ref().map_or(0, |ms| ms.creator_id as i32),
        map.checksum,
        map.version,
        map.seconds_total as i32,
        map.seconds_drain as i32,
        map.count_circles as i32,
        map.count_sliders as i32,
        map.count_spinners as i32,
        map.hp,
        map.cs,
        map.od,
        map.ar,
        map.mode as i16,
        map.status as i16,
        map.last_updated,
        map.stars,
        map.bpm,
    )
    .execute(&mut *conn)
    .await?;

    if let Some(ref mapset) = map.mapset {
        _insert_mapset(conn, mapset).await?;
    }

    Ok(())
}

fn _insert_mapset<'a>(
    conn: &'a mut PgConnection,
    mapset: &'a Beatmapset,
) -> BoxFuture<'a, BotResult<()>> {
    let fut = async move {
        sqlx::query!(
            "INSERT INTO mapsets (mapset_id,user_id,artist,title,creator,
            status,ranked_date,genre,language) VALUES
            ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (mapset_id) DO NOTHING",
            mapset.mapset_id as i32,
            mapset.creator_id as i32,
            mapset.artist,
            mapset.title,
            mapset.creator_name,
            mapset.status as i16,
            mapset.ranked_date,
            mapset.genre.map_or(1, |g| g as i16),
            mapset.language.map_or(1, |l| l as i16),
        )
        .execute(&mut *conn)
        .await?;

        if let Some(ref maps) = mapset.maps {
            for map in maps {
                _insert_map(&mut *conn, map).await?;
            }
        }

        Ok(())
    };

    fut.boxed()
}

fn _insert_mapset_compact<'a>(
    conn: &'a mut PgConnection,
    mapset: &'a BeatmapsetCompact,
    ranked_date: DateTime<Utc>,
) -> BoxFuture<'a, BotResult<()>> {
    let fut = async move {
        sqlx::query!(
            "INSERT INTO mapsets (mapset_id,user_id,artist,title,creator,
            status,ranked_date,genre,language) VALUES
            ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (mapset_id) DO NOTHING",
            mapset.mapset_id as i32,
            mapset.creator_id as i32,
            mapset.artist,
            mapset.title,
            mapset.creator_name,
            mapset.status as i16,
            ranked_date,
            mapset.genre.map_or(1, |g| g as i16),
            mapset.language.map_or(1, |l| l as i16),
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    };

    fut.boxed()
}
