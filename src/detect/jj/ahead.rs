use std::path::Path;
use std::process::Command;

pub fn get_ahead(path: &Path) -> Option<isize> {
    let bookmark_output = Command::new("jj")
        .arg("-R")
        .arg(path)
        .arg("log")
        .arg("-r")
        .arg("tracked_remote_bookmarks()")
        .arg("-T")
        .arg("bookmarks")
        .arg("-n")
        .arg("1")
        .arg("--no-pager")
        .arg("--no-graph")
        .output()
        .ok()?;

    if !bookmark_output.status.success() {
        return None;
    }

    let bookmark_text = String::from_utf8_lossy(&bookmark_output.stdout);
    if bookmark_text.trim().is_empty() {
        return None;
    }

    // TODO: If local is behind tracked remote bookmarks, report it as a negative number.
    let output = Command::new("jj")
        .arg("-R")
        .arg(path)
        .arg("log")
        .arg("-r")
        .arg("ancestors(@) ~ ancestors(tracked_remote_bookmarks())")
        .arg("-T")
        .arg("commit_id.short() ++ \"\\n\"")
        .arg("--no-pager")
        .arg("--no-graph")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ahead = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Some(ahead as isize)
}
