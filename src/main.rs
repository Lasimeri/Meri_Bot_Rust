mod meri_bot;
mod profilepfp;

#[tokio::main]
async fn main() {
    meri_bot::run().await;
}
