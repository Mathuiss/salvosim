#[derive(Debug)]
pub struct SalvoSimError {
    msg: String,
}

impl SalvoSimError {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

impl From<ConfigLoadError> for SalvoSimError {
    fn from(value: ConfigLoadError) -> Self {
        Self {
            msg: format!("{:?}", value),
        }
    }
}

#[derive(Debug)]
pub struct ConfigLoadError {
    msg: String,
}

impl From<std::io::Error> for ConfigLoadError {
    fn from(value: std::io::Error) -> Self {
        Self {
            msg: format!("{:?}", value),
        }
    }
}

impl From<toml::de::Error> for ConfigLoadError {
    fn from(value: toml::de::Error) -> Self {
        Self {
            msg: format!("{:?}", value),
        }
    }
}
