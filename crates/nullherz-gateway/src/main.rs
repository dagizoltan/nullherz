use nullherz_gateway::{connect_to_engine, run_gateway};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9001";
    let (cmd_prod, tel_cons, _cmd_buffer, _tel_prod) = connect_to_engine()?;

    // In standalone mode, we create a temporary DB. In conductor mode, this is passed.
    let lib_db = Arc::new(parking_lot::Mutex::new(nullherz_dna::LibraryDatabase::load("library.redb")?));

    run_gateway(addr, cmd_prod, tel_cons, Some(lib_db)).await
}
