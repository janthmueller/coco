use std::env;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CocoPaths {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub socket_path: PathBuf,
    pub codex_socket_path: PathBuf,
    pub worktrees_dir: PathBuf,
    pub codex_home: PathBuf,
}

#[derive(Debug, Error)]
pub enum PathError {
    #[error("HOME is not set and a required CoCo or Codex path has no explicit override")]
    MissingHome,
}

impl CocoPaths {
    pub fn from_env() -> Result<Self, PathError> {
        Self::resolve(|name| env::var_os(name).map(PathBuf::from))
    }

    fn resolve(mut get: impl FnMut(&str) -> Option<PathBuf>) -> Result<Self, PathError> {
        let data_dir = match get("COCO_DATA_DIR") {
            Some(path) => path,
            None => match get("XDG_DATA_HOME") {
                Some(path) => path.join("coco"),
                None => get("HOME")
                    .ok_or(PathError::MissingHome)?
                    .join(".local")
                    .join("share")
                    .join("coco"),
            },
        };
        let runtime_dir = get("XDG_RUNTIME_DIR")
            .map(|path| path.join("coco"))
            .unwrap_or_else(|| data_dir.clone());
        let codex_home = match get("CODEX_HOME") {
            Some(path) => path,
            None => get("HOME").ok_or(PathError::MissingHome)?.join(".codex"),
        };

        Ok(Self {
            database_path: get("COCO_DATABASE_PATH").unwrap_or_else(|| data_dir.join("coco.db")),
            socket_path: get("COCO_SOCKET_PATH").unwrap_or_else(|| runtime_dir.join("cocod.sock")),
            codex_socket_path: get("COCO_CODEX_SOCKET_PATH")
                .unwrap_or_else(|| runtime_dir.join("codex-app-server.sock")),
            worktrees_dir: get("COCO_WORKTREES_DIR").unwrap_or_else(|| data_dir.join("worktrees")),
            codex_home,
            data_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn resolves_xdg_defaults_and_independent_overrides() {
        let values = HashMap::from([
            ("HOME", "/home/test"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_RUNTIME_DIR", "/run/user/7"),
            ("COCO_DATABASE_PATH", "/private/state.db"),
        ]);
        let paths = CocoPaths::resolve(|name| {
            values
                .get(name)
                .map(|value| PathBuf::from(OsString::from(value)))
        })
        .unwrap();

        assert_eq!(paths.data_dir, PathBuf::from("/data/coco"));
        assert_eq!(paths.database_path, PathBuf::from("/private/state.db"));
        assert_eq!(
            paths.socket_path,
            PathBuf::from("/run/user/7/coco/cocod.sock")
        );
        assert_eq!(
            paths.codex_socket_path,
            PathBuf::from("/run/user/7/coco/codex-app-server.sock")
        );
        assert_eq!(paths.worktrees_dir, PathBuf::from("/data/coco/worktrees"));
        assert_eq!(paths.codex_home, PathBuf::from("/home/test/.codex"));
    }

    #[test]
    fn falls_back_to_home() {
        let paths = CocoPaths::resolve(|name| match name {
            "HOME" => Some(PathBuf::from("/home/test")),
            _ => None,
        })
        .unwrap();

        assert_eq!(
            paths.data_dir,
            PathBuf::from("/home/test/.local/share/coco")
        );
        assert_eq!(
            paths.socket_path,
            PathBuf::from("/home/test/.local/share/coco/cocod.sock")
        );
        assert_eq!(
            paths.codex_socket_path,
            PathBuf::from("/home/test/.local/share/coco/codex-app-server.sock")
        );
        assert_eq!(paths.codex_home, PathBuf::from("/home/test/.codex"));
    }

    #[test]
    fn explicit_data_and_codex_homes_do_not_require_home() {
        let paths = CocoPaths::resolve(|name| match name {
            "COCO_DATA_DIR" => Some(PathBuf::from("/state/coco")),
            "CODEX_HOME" => Some(PathBuf::from("/state/codex")),
            _ => None,
        })
        .unwrap();

        assert_eq!(paths.data_dir, PathBuf::from("/state/coco"));
        assert_eq!(paths.codex_home, PathBuf::from("/state/codex"));
    }
}
