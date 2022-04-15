use std::sync::Arc;

use rosu_v2::prelude::{Score, User};
use twilight_model::channel::Message;

use crate::{core::Context, embeds::NoChokeEmbed, BotResult};

use super::{Pages, Pagination};

pub struct NoChokePagination {
    ctx: Arc<Context>,
    msg: Message,
    pages: Pages,
    user: User,
    scores: Vec<(usize, Score, Score)>,
    unchoked_pp: f32,
    rank: Option<usize>,
}

impl NoChokePagination {
    pub fn new(
        msg: Message,
        user: User,
        scores: Vec<(usize, Score, Score)>,
        unchoked_pp: f32,
        rank: Option<usize>,
        ctx: Arc<Context>,
    ) -> Self {
        Self {
            msg,
            pages: Pages::new(5, scores.len()),
            user,
            scores,
            unchoked_pp,
            rank,
            ctx,
        }
    }
}

#[async_trait]
impl Pagination for NoChokePagination {
    type PageData = NoChokeEmbed;

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
        let fut = NoChokeEmbed::new(
            &self.user,
            self.scores
                .iter()
                .skip(self.pages.index)
                .take(self.pages.per_page),
            self.unchoked_pp,
            self.rank,
            &self.ctx,
            (self.page(), self.pages.total_pages),
        );

        Ok(fut.await)
    }
}
