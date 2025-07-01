mod meri_bot;
mod profilepfp;
mod lm;

#[tokio::main]
async fn main() {
    meri_bot::run().await;
}
