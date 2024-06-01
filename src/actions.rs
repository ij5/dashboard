use std::{ffi::OsStr, fs, path::Path};

use anyhow::Result;

#[derive(Debug, Default, Clone)]
pub struct Action {
    pub code: String,
    pub name: String,
}

pub fn initialize_scripts() -> Result<Vec<Action>> {
    let scripts = fs::read_dir("scripts")?;
    let mut actions = vec![];
    for filename in scripts {
        let filename = filename?;
        let filepath = filename.path();
        let path = filepath.to_str().unwrap();
        if !path.ends_with(".py") {
            continue;
        }
        let code = fs::read_to_string(path)?;
        let name = Path::new(path)
            .file_stem()
            .and_then(OsStr::to_str).unwrap();
        actions.push(Action {
            code,
            name: name.to_string(),
        })
    }
    Ok(actions)
}

