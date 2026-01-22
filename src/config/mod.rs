#[derive(Debug, Clone)]
pub struct AppConfig {
    pub debug: bool,
}

impl AppConfig {
    pub fn new(debug: bool) -> Self {
        let mut config = Self::default();
        config.debug = debug;
        config
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            debug: false,
        }
    }
}
