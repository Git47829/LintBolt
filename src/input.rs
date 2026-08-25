use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

pub fn discover(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        if path.as_os_str() == "-" {
            files.push(path.clone());
        } else if path.is_file() {
            if is_html(path) {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            let walker = WalkBuilder::new(path)
                .hidden(false)
                .follow_links(false)
                .build();
            for entry in walker {
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                        if is_html(entry.path()) {
                            files.push(entry.into_path());
                        }
                    }
                    Ok(_) => {}
                    Err(error) => errors.push((path.clone(), error.to_string())),
                }
            }
        } else {
            errors.push((path.clone(), "path does not exist".into()));
        }
    }

    files.sort();
    files.dedup();
    (files, errors)
}

fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_html_extensions() {
        assert!(is_html(Path::new("artifact.HTML")));
        assert!(is_html(Path::new("artifact.htm")));
        assert!(!is_html(Path::new("artifact.xml")));
    }
}
