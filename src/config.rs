use std::path::{Path, PathBuf};
use std::fs;
use serde::{Serialize, Deserialize};
use directories::{BaseDirs, ProjectDirs};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub app_home: PathBuf,
    pub default_notebook: String,
    pub editor: String,
    pub viewer: String,
}

const CONF_FILE: &str = "config.json";

pub fn load_configs() -> Config {
    match ProjectDirs::from("me", "clouds", "donno") {
        Some(project_dirs) => {
            let config_path = project_dirs.config_dir();
            let config_file_path = config_path.join(CONF_FILE);
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

pub fn print_config(key: &str) {
    let confs = load_configs();
    match key {
        "all" => println!("{:?}", confs),
        "app_home" => println!("{:?}", confs.app_home),
        "default_notebook" => println!("{:?}", confs.default_notebook),
        "editor" => println!("{:?}", confs.editor),
        "viewer" => println!("{:?}", confs.viewer),
        _ => println!("Invalid key name: {}", key),
    }
}

pub fn save_configs(conf: Config) {
    match ProjectDirs::from("me", "clouds", "donno") {
        Some(project_dirs) => {
            let config_path = project_dirs.config_dir();
            let config_file_path = config_path.join(CONF_FILE);
            if !Path::new(&config_file_path).exists() {
                match fs::create_dir_all(config_path) {
                    Ok(()) => (),
                    Err(error) => panic!("Mkdir failed: {:?}", error),
                }
            };
            let content = serde_json::to_string(&conf).unwrap();
            fs::write(&config_file_path, content)
                .unwrap_or_else(|_| panic!("Writing note file {} failed", &config_file_path.display()));
        },
        None => panic!("Get configuration file failed!"),
    }
}

pub fn set_config(kv: Vec<String>) {
    let confs = load_configs();
    let key: &str = &kv[0];
    let value: &str = &kv[1];

    let newconf = match key {
        "app_home" => Config { app_home: PathBuf::from(value), ..confs },
        "default_notebook" => Config {
            default_notebook: String::from(value), ..confs },
        "editor" => Config { editor: String::from(value), ..confs },
        "viewer" => Config { viewer: String::from(value), ..confs },
        _ => {
            println!("Invalid key name: {}", key);
            confs
        }
    };
    save_configs(newconf);
}
