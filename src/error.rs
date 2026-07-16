use std::fmt;

#[derive(Debug)]
pub struct SalvoSimError {
    msg: String,
}

impl fmt::Display for SalvoSimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for SalvoSimError {}

impl SalvoSimError {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

impl From<ConfigLoadError> for SalvoSimError {
    fn from(value: ConfigLoadError) -> Self {
        Self {
            msg: value.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ConfigLoadError {
    msg: String,
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for ConfigLoadError {}

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
