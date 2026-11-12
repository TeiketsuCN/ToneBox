fn main() -> anyhow::Result<()> {
    let found = fc_player::bluetooth::scan(8)?;
    println!("found: {}", found.len());
    for d in &found {
        println!("  {} | {}", d.name, d.id);
    }
    Ok(())
}
