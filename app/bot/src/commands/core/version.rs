use anyhow::Result;
use poise::{
    CreateReply,
    serenity_prelude::{CreateEmbed, CreateEmbedAuthor},
};

use crate::{bot::Context, discord::EmbedColor};

const IMAGE_URL: &str = "https://avatars.githubusercontent.com/u/46288749?v=4";

/// Shows the current active version of Spoticord
#[poise::command(slash_command)]
pub async fn version(ctx: Context<'_>) -> Result<()> {
    let description = format!(
        "Current version: {}\n\nSpoticord is open source, check it out [on GitHub](https://github.com/SpoticordMusic)",
        spoticord_config::VERSION
    );

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::default()
                .title("Spoticord Version")
                .author(
                    CreateEmbedAuthor::new("Maintained by: DaXcess (@daxcess)")
                        .url("https://github.com/DaXcess")
                        .icon_url(IMAGE_URL),
                )
                .description(description)
                .color(EmbedColor::Info),
        ),
    )
    .await?;

    Ok(())
}
