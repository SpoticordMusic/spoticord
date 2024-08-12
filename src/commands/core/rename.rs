use anyhow::Result;
use log::error;
use poise::CreateReply;
use serenity::all::{CreateEmbed, CreateEmbedFooter};
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;

use crate::bot::Context;

#[poise::command(slash_command)]
pub async fn rename(
    ctx: Context<'_>,

    #[description = "The new device name"]
    #[max_length = 32]
    #[min_length = 1]
    name: String,
) -> Result<()> {
    let db = ctx.data().database();

    let user = match db.get_or_create_user(ctx.author().id.to_string()).await {
        Ok(user) => user,
        Err(why) => {
            error!("Error fetching user: {why}");

            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .description("Something went wrong whilst trying to rename your Spoticord device.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;

            return Ok(());
        }
    };

    if let Err(why) = db.update_device_name(user.id, &name).await {
        error!("Error updating user device name: {why}");

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .description(
                            "Something went wrong while trying to rename your Spoticord device.",
                        )
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    let has_session = ctx
        .data()
        .get_session(SessionQuery::Owner(ctx.author().id))
        .is_some();

    ctx.send(
        CreateReply::default()
            .embed({
                let mut embed = CreateEmbed::new()
                    .description(format!(
                        "Successfully changed the Spotify device name to **{}**",
                        spoticord_utils::discord::escape(name)
                    ))
                    .color(Colors::Success);

                if has_session {
                    embed = embed.footer(CreateEmbedFooter::new(
                        "You must reconnect the player for the new name to show up",
                    ));
                }

                embed
            })
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
