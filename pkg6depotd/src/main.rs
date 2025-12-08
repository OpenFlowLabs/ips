use pkg6depotd::run;
use miette::Result;

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}
