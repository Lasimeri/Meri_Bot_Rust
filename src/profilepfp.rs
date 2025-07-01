use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::{channel::Message, channel::AttachmentType, id::UserId, Timestamp},
};

#[command]
#[aliases("profilepic", "avatar", "pfp")]
pub async fn ppfp(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let args_str = args.message().trim();
    
    // Check if a user was mentioned
    if args_str.is_empty() {
        msg.reply(ctx, "❌ Please mention a user! Usage: `^ppfp @user`").await?;
        return Ok(());
    }

    // Parse the user mention - extract user ID from mention format <@123456789>
    let user_id = if args_str.starts_with("<@") && args_str.ends_with(">") {
        let id_str = args_str.trim_start_matches("<@").trim_end_matches(">").trim_start_matches("!");
        match id_str.parse::<u64>() {
            Ok(id) => UserId(id),
            Err(_) => {
                msg.reply(ctx, "❌ Invalid user mention! Please use `^ppfp @user`").await?;
                return Ok(());
            }
        }
    } else {
        msg.reply(ctx, "❌ Invalid user mention! Please use `^ppfp @user`").await?;
        return Ok(());
    };

    // Get the user
    let user = match ctx.http.get_user(user_id.into()).await {
        Ok(user) => user,
        Err(_) => {
            msg.reply(ctx, "❌ User not found!").await?;
            return Ok(());
        }
    };

    // Get the user's avatar URL
    let avatar_url = match user.avatar_url() {
        Some(url) => url,
        None => {
            // If no custom avatar, use default avatar
            user.default_avatar_url()
        }
    };

    // Download the image data
    let image_data = match download_image(&avatar_url).await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("❌ Failed to download avatar: {}", e);
            msg.reply(ctx, "❌ Failed to download the profile picture!").await?;
            return Ok(());
        }
    };

    // Determine file extension from URL
    let file_extension = if avatar_url.contains(".gif") {
        "gif"
    } else if avatar_url.contains(".webp") {
        "webp" 
    } else if avatar_url.contains(".jpg") {
        "jpg"
    } else {
        "png" // Default to PNG
    };

    let filename = format!("{}_avatar.{}", user.name, file_extension);

    // Create attachment from image data in memory
    let attachment = AttachmentType::Bytes {
        data: image_data.into(),
        filename: filename.clone(),
    };

    // Send the message with embed and attachment
    if let Err(e) = msg.channel_id.send_message(&ctx.http, |m| {
        m.add_file(attachment);
        m.embed(|e| {
            e.title(format!("{}'s Profile Picture", user.name));
            e.description(format!("**User:** <@{}>", user.id));
            e.color(0x7289DA); // Discord blurple color
            e.image(format!("attachment://{}", filename));
            e.url(&avatar_url); // Make the title clickable to the original image
                         e.footer(|f| f.text(format!("Requested by {}", msg.author.name)));
             e.timestamp(Timestamp::now());
            e
        });
        m
    }).await {
        eprintln!("❌ Failed to send message: {}", e);
        msg.reply(ctx, "❌ Failed to send the profile picture!").await?;
    }

    Ok(())
}

// Helper function to download image data into memory
async fn download_image(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
} 