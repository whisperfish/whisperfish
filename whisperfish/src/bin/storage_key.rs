use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let password = std::env::args().nth(1).context("storage_key [password]")?;
    let salt = std::fs::read("salt").context("execute this program in the `db` subdirectory")?;

    // Derive database key
    let params = scrypt::Params::new(14, 8, 1, 32).unwrap();
    let mut key_database = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key_database)
        .context("Cannot compute database key")?;
    println!("Database key: {:?}", hex::encode(key_database));

    Ok(())
}
