//! Server configuration from env (see docs/api.md "Server env").

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub database_url: String,
    pub api_key: String,
    pub bind: String,
    pub cors_origin: Option<String>,
    pub engine: aegis_core::Config,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let engine = match var("AEGIS_CONFIG") {
            Some(path) => {
                let s = std::fs::read_to_string(&path).with_context(|| format!("reading AEGIS_CONFIG {path}"))?;
                aegis_core::Config::from_toml(&s)?
            }
            None => aegis_core::Config::default(),
        };
        Ok(Self {
            database_url: var("DATABASE_URL").context("DATABASE_URL is required")?,
            api_key: var("AEGIS_API_KEY").context("AEGIS_API_KEY is required")?,
            bind: var("AEGIS_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            cors_origin: var("AEGIS_CORS_ORIGIN"),
            engine,
        })
    }
}
