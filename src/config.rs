use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::redis_client::RateLimitAlgorithm;

/// レートリミットの設定を保持する構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitSettings {
    /// Redisサーバーの接続URL
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    /// レート制限に使用するキー（remote_addr、http_x_api_keyなど）
    #[serde(default = "default_key")]
    pub key: String,

    /// 1秒あたりの最大リクエスト数
    #[serde(default = "default_rate")]
    pub rate: u32,

    /// 一時的に許容される超過リクエスト数
    #[serde(default = "default_burst")]
    pub burst: u32,

    /// レート制限アルゴリズム
    #[serde(default = "default_algorithm")]
    pub algorithm: String,

    /// 時間窓のサイズ（秒）
    #[serde(default = "default_window_size")]
    pub window_size: u32,

    /// モジュールの有効/無効
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            redis_url: default_redis_url(),
            key: default_key(),
            rate: default_rate(),
            burst: default_burst(),
            algorithm: default_algorithm(),
            window_size: default_window_size(),
            enabled: default_enabled(),
        }
    }
}

/// LocationごとのRateLimitSettingsマップ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    /// デフォルト設定（全てのLocationで共有される）
    #[serde(default)]
    pub default: RateLimitSettings,

    /// Locationごとの設定（デフォルト設定をオーバーライドする）
    #[serde(default)]
    pub locations: HashMap<String, RateLimitSettings>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            default: RateLimitSettings::default(),
            locations: HashMap::new(),
        }
    }
}

impl ConfigFile {
    /// ファイルから設定を読み込む
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file_path = path.as_ref();
        info!("Loading configuration from file: {:?}", file_path);

        let mut file = match File::open(file_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open config file: {}", e);
                return Err(format!("Failed to open config file: {}", e));
            }
        };

        let mut contents = String::new();
        if let Err(e) = file.read_to_string(&mut contents) {
            error!("Failed to read config file: {}", e);
            return Err(format!("Failed to read config file: {}", e));
        }

        match serde_json::from_str(&contents) {
            Ok(config) => Ok(config),
            Err(e) => {
                error!("Failed to parse config file: {}", e);
                Err(format!("Failed to parse config file: {}", e))
            }
        }
    }

    /// 特定のLocationの設定を取得する。Locationが設定されていない場合はデフォルト設定を返す
    pub fn get_settings(&self, location: &str) -> RateLimitSettings {
        if let Some(location_settings) = self.locations.get(location) {
            location_settings.clone()
        } else {
            self.default.clone()
        }
    }

    /// 設定からRateLimitAlgorithmを解析する
    pub fn parse_algorithm(algorithm_str: &str) -> Result<RateLimitAlgorithm, String> {
        RateLimitAlgorithm::from_str(algorithm_str)
    }
}

// デフォルト値関数
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_key() -> String {
    "remote_addr".to_string()
}

fn default_rate() -> u32 {
    10
}

fn default_burst() -> u32 {
    5
}

fn default_algorithm() -> String {
    "sliding_window".to_string()
}

fn default_window_size() -> u32 {
    60
}

fn default_enabled() -> bool {
    false
}
