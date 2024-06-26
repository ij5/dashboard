use std::fs::OpenOptions;
use std::io::Write;
use color_eyre::eyre::Result;

pub fn println(output: &str) -> Result<()> {
    let mut w = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open("run.log")?;
    writeln!(&mut w, "{}", output)?;
    Ok(())
}
