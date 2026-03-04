use std::path::Path;

use super::jj;
use super::repo::{RepoInfo, RepoType};

pub fn get_ahead(path: &Path, info: &RepoInfo) -> Option<isize> {
    match info.repo_type {
        RepoType::Jujutsu => jj::get_ahead(path),
        RepoType::Git => {
            // TODO: Add Git ahead detection.
            None
        }
        RepoType::None => None,
    }
}
