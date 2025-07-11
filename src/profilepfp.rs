use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::{channel::Message, channel::AttachmentType, id::UserId, Timestamp},
};

#[command]
#[aliases("avatar", "pfp", "profilepic")]
pub async fn ppfp(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    let mentioned_user = if let Some(user) = msg.mentions.first() {
        user
    } else {
        msg.reply(ctx, "Please mention a user! Usage: `^ppfp @user`").await?;
        return Ok(());
    };

    let avatar_url = match mentioned_user.avatar_url() {
        Some(url) => url,
        None => {
            msg.reply(ctx, "User does not have a profile picture.").await?;
            return Ok(());
        }
    };
    
    // Append size parameter for higher resolution
    let high_res_url = format!("{}?size=4096", avatar_url);

    match download_image(&high_res_url).await {
        Ok(image_data) => {
            let filename = if high_res_url.contains(".gif") {
                "avatar.gif"
            } else {
                "avatar.png"
            };

            if let Err(e) = msg.channel_id.send_message(&ctx.http, |m| {
                m.embed(|e| {
                    e.title(&format!("{}'s Profile Picture", mentioned_user.name))
                     .url(&high_res_url)
                     .image(&format!("attachment://{}", filename))
                     .color(0x00BFFF) // Deep sky blue
                     .footer(|f| {
                         f.text(&format!("Requested by {}", msg.author.name))
                          .icon_url(msg.author.face())
                     })
                     .timestamp(chrono::Utc::now())
                })
                .add_file((image_data.as_slice(), filename))
            }).await {
                eprintln!("Failed to send profile picture embed: {}", e);
                msg.reply(ctx, "Failed to send profile picture.").await?;
            }
        },
        Err(e) => {
            eprintln!("Failed to download avatar image: {}", e);
            msg.reply(ctx, "Failed to download user's profile picture.").await?;
        }
    }
    
    Ok(())
}

// Helper function to download image to memory with proper headers
async fn download_image(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to download image: HTTP {}", response.status()).into());
    }
    
    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
} 