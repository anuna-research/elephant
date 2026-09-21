//! Bundled agent guidance, available without opening an Elephant store.

use crate::errors::{AppError, AppResult};
use std::{fs, io::Write, path::Path};

const TEMPLATE: &str = include_str!("../skills/elephant/SKILL.md");

pub fn init(dir: &Path, dry_run: bool, json: bool) -> AppResult<()> {
    let content = TEMPLATE.replace("{{ELEPHANT_VERSION}}", crate::VERSION);
    let path = dir.join("SKILL.md");
    let status = if dry_run {
        "preview"
    } else {
        fs::create_dir_all(dir)?;
        // Exclusive creation prevents overwriting edits, including a racing writer.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(content.as_bytes())?;
                "installed"
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                    return Err(AppError::Config(format!(
                        "{} already exists with different content; use --dry-run to review the bundled skill or --path to install elsewhere",
                        path.display()
                    )));
                }
                "unchanged"
            }
            Err(e) => return Err(e.into()),
        }
    };
    if json {
        let mut result = serde_json::json!({
            "status": status, "path": path, "version": crate::VERSION,
        });
        if dry_run {
            result["content"] = content.into();
        }
        println!("{result}");
    } else if dry_run {
        print!("{content}");
    } else {
        println!("{status}: {}", path.display());
    }
    Ok(())
}
