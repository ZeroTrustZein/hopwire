//! Server configuration from env (see docs/api.md "Server env").

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub database_url: String,
    pub api_key: String,
    pub bind: String,
    pub cors_origin: Option<String>,
    pub engine: hopwire_core::Config,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let engine = match var("HOPWIRE_CONFIG") {
            Some(path) => {
                let s = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading HOPWIRE_CONFIG {path}"))?;
                hopwire_core::Config::from_toml(&s)?
            }
            None => hopwire_core::Config::default(),
        };
        Ok(Self {
            database_url: var("DATABASE_URL").context("DATABASE_URL is required")?,
            api_key: var("HOPWIRE_API_KEY").context("HOPWIRE_API_KEY is required")?,
            bind: var("HOPWIRE_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            cors_origin: var("HOPWIRE_CORS_ORIGIN"),
            engine,
        })
    }
}
