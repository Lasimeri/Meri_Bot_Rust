use serenity::{
    client::Context,
    framework::standard::{macros::command, Args, CommandResult},
    model::channel::Message,
};

#[command]
pub async fn echo(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let _typing = ctx.http.start_typing(msg.channel_id.0)?;
    let text = args.message();
    if text.is_empty() {
        msg.reply(ctx, "Please provide text to echo!").await?;
    } else {
        msg.reply(ctx, text).await?;
    }
    Ok(())
} 