use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::redis_client::{RateLimitAlgorithm, RedisConnectionOptions};

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

    /// Redis接続オプション
    #[serde(default)]
    pub redis_options: RedisConnectionOptions,
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
            redis_options: RedisConnectionOptions::default(),
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
            // ロケーション固有の設定がある場合、デフォルト値から足りない項目を継承
            let mut merged_settings = self.default.clone();

            // デフォルト値が上書きされている項目のみを更新
            if location_settings.redis_url != default_redis_url() {
                merged_settings.redis_url = location_settings.redis_url.clone();
            }

            if location_settings.key != default_key() {
                merged_settings.key = location_settings.key.clone();
            }

            if location_settings.rate != default_rate() {
                merged_settings.rate = location_settings.rate;
            }

            if location_settings.burst != default_burst() {
                merged_settings.burst = location_settings.burst;
            }

            if location_settings.algorithm != default_algorithm() {
                merged_settings.algorithm = location_settings.algorithm.clone();
            }

            if location_settings.window_size != default_window_size() {
                merged_settings.window_size = location_settings.window_size;
            }

            // 有効/無効フラグは明示的に設定されている場合のみ上書き
            if location_settings.enabled != self.default.enabled {
                merged_settings.enabled = location_settings.enabled;
            }

            // Redis接続オプションをマージ（設定されている項目のみを上書き）
            // 注: デフォルト値と異なる項目のみをマージ
            merge_redis_options(
                &mut merged_settings.redis_options,
                &location_settings.redis_options,
            );

            merged_settings
        } else {
            self.default.clone()
        }
    }

    /// 設定からRateLimitAlgorithmを解析する
    pub fn parse_algorithm(algorithm_str: &str) -> Result<RateLimitAlgorithm, String> {
        RateLimitAlgorithm::from_str(algorithm_str)
    }
}

/// Redis接続オプションをマージする（srcにある非デフォルト値のみをdestに適用）
fn merge_redis_options(dest: &mut RedisConnectionOptions, src: &RedisConnectionOptions) {
    // デフォルト値と異なる接続タイムアウトのみを適用
    if src.connect_timeout != RedisConnectionOptions::default().connect_timeout {
        dest.connect_timeout = src.connect_timeout;
    }

    // デフォルト値と異なるコマンドタイムアウトのみを適用
    if src.command_timeout != RedisConnectionOptions::default().command_timeout {
        dest.command_timeout = src.command_timeout;
    }

    // デフォルト値と異なるリトライ回数のみを適用
    if src.retry_count != RedisConnectionOptions::default().retry_count {
        dest.retry_count = src.retry_count;
    }

    // デフォルト値と異なるリトライ間隔のみを適用
    if src.retry_delay != RedisConnectionOptions::default().retry_delay {
        dest.retry_delay = src.retry_delay;
    }

    // パスワードが設定されている場合のみ適用
    if src.password.is_some() {
        dest.password = src.password.clone();
    }

    // デフォルト値と異なるデータベース番号のみを適用
    if src.database != RedisConnectionOptions::default().database {
        dest.database = src.database;
    }

    // デフォルト値と異なる接続プールサイズのみを適用
    if src.pool_size != RedisConnectionOptions::default().pool_size {
        dest.pool_size = src.pool_size;
    }

    // クラスタモードの設定
    if src.cluster_mode != RedisConnectionOptions::default().cluster_mode {
        dest.cluster_mode = src.cluster_mode;
    }

    // TLS設定
    if src.tls_enabled != RedisConnectionOptions::default().tls_enabled {
        dest.tls_enabled = src.tls_enabled;
    }

    // キープアライブ設定
    if src.keepalive != RedisConnectionOptions::default().keepalive {
        dest.keepalive = src.keepalive;
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
