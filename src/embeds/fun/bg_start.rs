use crate::embeds::EmbedData;

use twilight_model::id::UserId;

pub struct BGStartEmbed {
    description: Option<String>,
}

impl BGStartEmbed {
    pub fn new(author: UserId) -> Self {
        let description = format!(
            "**React to include tag, unreact to exclude tag.**\n\
            <@{}> react with ✅ when you're ready.\n\
            ```\n\
            🍋: Easy 🎨: Weeb 😱: Hard name 🗽: English 💯: Tech\n\
            🤓: Hard 🍨: Kpop 🪀: Alternate 🌀: Streams ✅: Lock in\n\
            🤡: Meme 👨‍🌾: Farm 🟦: Blue sky  👴: Old     ❌: Abort\n\
            ```",
            author
        );
        Self {
            description: Some(description),
        }
    }
}

impl EmbedData for BGStartEmbed {
    fn description_owned(&mut self) -> Option<String> {
        self.description.take()
    }
}
