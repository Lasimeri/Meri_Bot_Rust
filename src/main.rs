mod meri_bot;

#[tokio::main]
async fn main() {
    meri_bot::run().await;
}
