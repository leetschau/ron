use std::path::{Path, PathBuf};
use std::fs;
use serde::{Serialize, Deserialize};
use directories::{BaseDirs, ProjectDirs};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    app_home: PathBuf,
    pub default_notebook: String,
    pub editor: String,
    pub viewer: String,
}

pub fn load_configs() -> Config {
    match ProjectDirs::from("me", "clouds", "donno") {
        Some(project_dirs) => {
            let config_path = project_dirs.config_dir();
            let config_file_path = config_path.join("config.json");
            if Path::new(&config_file_path).exists() {
                let raw = fs::read_to_string(config_file_path).expect("Unable to read file");
                serde_json::from_str(&raw).expect("JSON was not well-formatted")
            } else {
                let default_conf = Config {
                    app_home: BaseDirs::new().unwrap().home_dir().join(".donno/"),
                    default_notebook: String::from("/Misc"),
                    editor: String::from("nvim"),
                    viewer: String::from("nvim -R"),
                };
                let content = serde_json::to_string(&default_conf).unwrap();
                println!("Write configuration file to {:?}", config_file_path);
                match fs::create_dir_all(config_path) {
                    Ok(()) => (),
                    Err(error) => panic!("Mkdir failed: {:?}", error),
                }
                fs::write(config_file_path, content)
                    .expect("Unable to write file");
                default_conf
            }
        },
        None => panic!("Get configuration file failed!"),
    }
}
