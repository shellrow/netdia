use std::path::PathBuf;

pub const USER_APP_DIR_NAME: &str = ".netdia";

pub fn get_app_dir_path() -> Option<PathBuf> {
    match home::home_dir() {
        Some(mut path) => {
            path.push(USER_APP_DIR_NAME);
            if !path.exists() {
                match std::fs::create_dir_all(&path) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Failed to create config dir: {:?}", e);
                        return None;
                    }
                }
            }
            Some(path)
        }
        None => None,
    }
}

pub fn get_user_file_path(file_name: &str) -> Option<PathBuf> {
    match get_app_dir_path() {
        Some(mut path) => {
            path.push(file_name);
            Some(path)
        }
        None => None,
    }
}
