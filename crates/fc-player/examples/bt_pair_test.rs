fn main() -> anyhow::Result<()> {
    for paired in [true, false] {
        let list = fc_player::bluetooth::list(paired)?;
        println!("paired={}: {}", paired, list.len());
        for d in &list { println!("  {}", d.name); }
    }
    Ok(())
}