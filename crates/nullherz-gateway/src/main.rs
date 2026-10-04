use nullherz_gateway::{connect_to_engine, run_gateway};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9001";
    let (cmd_prod, tel_cons, _cmd_buffer, _tel_prod) = connect_to_engine()?;

    let db_path = if std::path::Path::new("storage/db/library.redb").exists() {
        "storage/db/library.redb"
    } else if std::path::Path::new("library.redb").exists() {
        "library.redb"
    } else {
        let _ = std::fs::create_dir_all("storage/db");
        "storage/db/library.redb"
    };
    let lib_db = Arc::new(parking_lot::Mutex::new(nullherz_dna::LibraryDatabase::load(db_path)?));

    run_gateway(addr, cmd_prod, tel_cons, Some(lib_db)).await
}
