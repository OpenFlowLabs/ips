use miette::Result;
use pkg6depotd::run;

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}
