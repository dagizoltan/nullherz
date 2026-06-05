use nullherz_gateway::{connect_to_engine, run_gateway};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9001";
    let (cmd_prod, tel_cons) = connect_to_engine()?;
    run_gateway(addr, cmd_prod, tel_cons).await
}
