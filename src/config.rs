use serde::Deserialize;
use toml::Table;

#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) access_token: String,
    pub(crate) url: String,
}

const ACCESS_TOKEN: &str = "access_token";
const URL: &str = "url";

impl Config {
    pub(crate) fn new(file_path: &str) -> Self {
        let c = std::fs::read_to_string(file_path)
            .expect("unable to open config file")
            .parse::<Table>()
            .expect("unable to parse config file");
        Config {
            access_token: get(&c, ACCESS_TOKEN),
            url: get(&c, URL),
        }
    }
}

fn get(t: &Table, k: &str) -> String {
    t.get(k)
        .unwrap_or_else(|| panic!("config does not contain {}", k))
        .as_str()
        .unwrap_or_else(|| panic!("unable to parse {}", k))
        .to_owned()
}
