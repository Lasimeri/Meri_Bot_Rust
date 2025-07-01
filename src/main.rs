mod meri_bot;
mod profilepfp;
mod lm;
mod reason;

#[tokio::main]
async fn main() {
    meri_bot::run().await;
}
