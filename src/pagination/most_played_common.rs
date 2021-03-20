use super::{Pages, Pagination};

use crate::{embeds::MostPlayedCommonEmbed, BotResult};

use async_trait::async_trait;
use rosu_v2::prelude::MostPlayedMap;
use std::collections::HashMap;
use twilight_model::channel::Message;

pub struct MostPlayedCommonPagination {
    msg: Message,
    pages: Pages,
    names: Vec<String>,
    users_count: Vec<HashMap<u32, usize>>,
    maps: Vec<MostPlayedMap>,
}

impl MostPlayedCommonPagination {
    pub fn new(
        msg: Message,
        names: Vec<String>,
        users_count: Vec<HashMap<u32, usize>>,
        maps: Vec<MostPlayedMap>,
    ) -> Self {
        Self {
            pages: Pages::new(10, maps.len()),
            msg,
            names,
            users_count,
            maps,
        }
    }
}

#[async_trait]
impl Pagination for MostPlayedCommonPagination {
    type PageData = MostPlayedCommonEmbed;

    fn msg(&self) -> &Message {
        &self.msg
    }

    fn pages(&self) -> Pages {
        self.pages
    }

    fn pages_mut(&mut self) -> &mut Pages {
        &mut self.pages
    }

    fn single_step(&self) -> usize {
        self.pages.per_page
    }

    async fn build_page(&mut self) -> BotResult<Self::PageData> {
        Ok(MostPlayedCommonEmbed::new(
            &self.names,
            &self.maps[self.pages.index..(self.pages.index + 10).min(self.maps.len())],
            &self.users_count,
            self.pages.index,
        ))
    }
}
