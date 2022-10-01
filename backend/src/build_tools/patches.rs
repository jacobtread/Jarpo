use async_walkdir::WalkDir;
use futures::StreamExt;
use log::{info, warn};
use patch::Patch;
use std::fs::read;
use std::path::PathBuf;
use std::{io, path::Path};
use tokio::task::{spawn_blocking, JoinError};

use crate::define_from_value;

#[derive(Debug)]
pub enum PatchError {
    IO(io::Error),
    Join(JoinError),
    Parse,
    MissingFile(PathBuf),
    InvalidFile,
}

define_from_value! {
  PatchError {
    IO = io::Error,
    Join = JoinError,
  }
}

type PatchResult<T> = Result<T, PatchError>;

pub async fn apply_patches(patches: PathBuf, target: PathBuf) -> PatchResult<()> {
    let mut count: usize = 0;
    let mut walk = WalkDir::new(patches);
    while let Some(entry) = walk.next().await {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name
            .as_ref()
            .ends_with(".patch")
        {
            count += 1;
        }
    }

    info!("Total patches {count}");

    Ok(())
}

fn apply_patches_for(current: PathBuf, target: &PathBuf) -> PatchResult<()> {
    use std::fs::{read, read_dir};

    let current = current;
    let rd = read_dir(&current)?;

    let mut count = 0;

    for entry in rd {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let ftype = entry.file_type()?;
        let file_path = current.join(name.as_ref());
        if ftype.is_dir() {
            apply_patches_for(file_path, target)?;
        } else {
            if !name
                .as_ref()
                .ends_with(".patch")
            {
                continue;
            }

            count += 1;

            let patch = read(&file_path)?;
            let patch = String::from_utf8_lossy(&patch).replace("\r\n", "\n");
            let patch = Patch::from_single(&patch).unwrap();
            let old_file = &patch.old.path;
            let file_path = &old_file[2..];
            let target_path = target.join(file_path);
            match apply_patch(patch, &target) {
                Ok(_) => {}
                Err(err) => {}
            }
        }
    }

    info!("Dir at {current:?} found {count} patches");

    Ok(())
}

fn apply_patch(patch: Patch, target_path: &PathBuf) -> PatchResult<()> {
    let old_path = &patch.old.path;
    if old_path.len() < 3 {
        return Err(PatchError::InvalidFile);
    }
    let old_path = &old_path[2..];
    let path = target_path.join(old_path);

    if !path.exists() {
        return Err(PatchError::MissingFile(path));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::build_tools::patches::{apply_patches, PatchResult};
    use log::error;
    use std::path::Path;

    #[tokio::test]
    async fn test() {
        dotenv::dotenv().ok();
        env_logger::init();
        let build = Path::new("build");
        let patches = build.join("craftbukkit/nms-patches");
        let target = build.join("work/decompile-0bc44701");

        apply_patches(patches, target)
            .await
            .unwrap();
    }
}
