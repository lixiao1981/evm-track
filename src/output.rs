use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use std::time::{SystemTime, UNIX_EPOCH};

/// 输出格式枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    JsonLines,
    Csv,
    Console,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Console
    }
}

/// 输出配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub file_path: Option<PathBuf>,
    pub rotate_size_mb: Option<u64>,
    pub compress: bool,
    pub buffer_size: usize,
    pub auto_flush_interval_secs: u64,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Console,
            file_path: None,
            rotate_size_mb: Some(100), // 100MB轮转
            compress: false,
            buffer_size: 100,
            auto_flush_interval_secs: 30,
        }
    }
}

/// 检测结果严重程度
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// 标准化的检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub timestamp: u64,
    pub block_number: Option<u64>,
    pub tx_hash: Option<String>,
    pub tx_index: Option<u64>,
    pub log_index: Option<u64>,
    pub action_type: String,
    pub event_type: String,
    pub contract_address: Option<String>,
    pub data: serde_json::Value,
    pub severity: Severity,
    pub tags: Vec<String>,
}

impl DetectionResult {
    /// 创建新的检测结果
    pub fn new(
        action_type: String,
        event_type: String,
        data: serde_json::Value,
        severity: Severity,
    ) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            block_number: None,
            tx_hash: None,
            tx_index: None,
            log_index: None,
            action_type,
            event_type,
            contract_address: None,
            data,
            severity,
            tags: Vec::new(),
        }
    }

    /// 设置区块信息
    pub fn with_block_info(mut self, block_number: Option<u64>) -> Self {
        self.block_number = block_number;
        self
    }

    /// 设置交易信息
    pub fn with_tx_info(mut self, tx_hash: Option<String>, tx_index: Option<u64>) -> Self {
        self.tx_hash = tx_hash;
        self.tx_index = tx_index;
        self
    }

    /// 设置日志信息
    pub fn with_log_info(mut self, log_index: Option<u64>) -> Self {
        self.log_index = log_index;
        self
    }

    /// 设置合约地址
    pub fn with_contract_address(mut self, address: Option<String>) -> Self {
        self.contract_address = address;
        self
    }

    /// 添加标签
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// 添加单个标签
    pub fn add_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }
}

/// 输出管理器
pub struct OutputManager {
    config: OutputConfig,
    buffer: Vec<DetectionResult>,
    file_handle: Option<tokio::fs::File>,
    current_file_size: u64,
    file_counter: u32,
}

impl OutputManager {
    /// 创建新的输出管理器
    pub async fn new(config: OutputConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let (file_handle, current_file_size) = if let Some(path) = &config.file_path {
            // 检查文件是否存在以获取当前大小
            let current_size = if path.exists() {
                tokio::fs::metadata(path).await?.len()
            } else {
                0
            };

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?;

            (Some(file), current_size)
        } else {
            (None, 0)
        };

        info!("📁 Output manager initialized: format={:?}, file={:?}", 
            config.format, config.file_path);

        let buffer_size = config.buffer_size;
        Ok(Self {
            config,
            buffer: Vec::with_capacity(buffer_size),
            file_handle,
            current_file_size,
            file_counter: 0,
        })
    }

    /// 保存检测结果
    pub async fn save_result(&mut self, result: DetectionResult) -> Result<(), Box<dyn std::error::Error>> {
        debug!("💾 Saving detection result: action={}, event={}", 
            result.action_type, result.event_type);

        // 同时输出到控制台（如果配置了）
        if matches!(self.config.format, OutputFormat::Console) || self.file_handle.is_none() {
            self.print_to_console(&result);
        }

        // 如果没有文件输出配置，只输出到控制台
        if self.file_handle.is_none() {
            return Ok(());
        }

        // 添加到缓冲区
        self.buffer.push(result);

        // 检查是否需要刷新缓冲区
        if self.buffer.len() >= self.config.buffer_size {
            self.flush().await?;
        }

        Ok(())
    }

    /// 刷新缓冲区到文件
    pub async fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.buffer.is_empty() || self.file_handle.is_none() {
            return Ok(());
        }

        debug!("🔄 Flushing {} results to file", self.buffer.len());

        // 检查是否需要轮转文件
        if let Some(max_size_mb) = self.config.rotate_size_mb {
            let max_size_bytes = max_size_mb * 1024 * 1024;
            if self.current_file_size > max_size_bytes {
                self.rotate_file().await?;
            }
        }

        let file = self.file_handle.as_mut().unwrap();
        let mut content = Vec::new();

        match self.config.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&self.buffer)?;
                content = json.into_bytes();
            }
            OutputFormat::JsonLines => {
                for result in &self.buffer {
                    let line = serde_json::to_string(result)?;
                    content.extend_from_slice(format!("{}\n", line).as_bytes());
                }
            }
            OutputFormat::Csv => {
                // CSV头部（如果文件为空）
                if self.current_file_size == 0 {
                    let header = "timestamp,block_number,tx_hash,tx_index,log_index,action_type,event_type,contract_address,severity,tags,data\n";
                    content.extend_from_slice(header.as_bytes());
                }

                for result in &self.buffer {
                    let csv_line = format!(
                        "{},{},{},{},{},{},{},{},{},{},\"{}\"\n",
                        result.timestamp,
                        result.block_number.unwrap_or(0),
                        result.tx_hash.as_deref().unwrap_or(""),
                        result.tx_index.unwrap_or(0),
                        result.log_index.unwrap_or(0),
                        result.action_type,
                        result.event_type,
                        result.contract_address.as_deref().unwrap_or(""),
                        serde_json::to_string(&result.severity)?.trim_matches('"'),
                        result.tags.join(";"),
                        result.data.to_string().replace("\"", "\\\"")
                    );
                    content.extend_from_slice(csv_line.as_bytes());
                }
            }
            OutputFormat::Console => {
                // 控制台输出已在save_result中处理
                return Ok(());
            }
        }

        file.write_all(&content).await?;
        file.flush().await?;
        
        self.current_file_size += content.len() as u64;
        let buffer_count = self.buffer.len();
        self.buffer.clear();

        debug!("✅ Flushed {} results, file size: {} bytes", buffer_count, self.current_file_size);
        Ok(())
    }

    /// 轮转文件
    async fn rotate_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = &self.config.file_path {
            self.file_counter += 1;
            
            let mut new_path = path.clone();
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let ext = path.extension().unwrap_or_default().to_string_lossy();
            
            let new_filename = if ext.is_empty() {
                format!("{}.{}", stem, self.file_counter)
            } else {
                format!("{}.{}.{}", stem, self.file_counter, ext)
            };
            
            new_path.set_file_name(new_filename);
            
            info!("🔄 Rotating file: {} -> {}", path.display(), new_path.display());
            
            // 关闭当前文件
            drop(self.file_handle.take());
            
            // 重命名当前文件
            tokio::fs::rename(path, &new_path).await?;
            
            // 创建新文件
            self.file_handle = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?
            );
            
            self.current_file_size = 0;
            
            // 如果启用压缩
            if self.config.compress {
                // TODO: 实现文件压缩
                info!("🗜️  File compression not implemented yet");
            }
        }
        
        Ok(())
    }

    /// 输出到控制台
    fn print_to_console(&self, result: &DetectionResult) {
        let severity_icon = match result.severity {
            Severity::Info => "ℹ️",
            Severity::Warning => "⚠️",
            Severity::Critical => "🚨",
        };

        let timestamp = chrono::DateTime::from_timestamp(result.timestamp as i64, 0)
            .unwrap_or_default()
            .format("%H:%M:%S");

        let block_info = if let Some(block) = result.block_number {
            format!("block={}", block)
        } else {
            "block=pending".to_string()
        };

        let tx_info = if let Some(tx) = &result.tx_hash {
            format!("tx={}...{}", &tx[..10], &tx[tx.len()-8..])
        } else {
            "tx=N/A".to_string()
        };

        let contract_info = if let Some(addr) = &result.contract_address {
            format!("contract={}...{}", &addr[..10], &addr[addr.len()-8..])
        } else {
            String::new()
        };

        let tags_info = if !result.tags.is_empty() {
            format!(" [{}]", result.tags.join(","))
        } else {
            String::new()
        };

        println!(
            "{} [{}] {} [{}] {} {} {} {}{}",
            severity_icon,
            timestamp,
            result.action_type,
            result.event_type,
            block_info,
            tx_info,
            contract_info,
            result.data,
            tags_info
        );
    }

    /// 获取统计信息
    pub fn stats(&self) -> OutputStats {
        OutputStats {
            buffer_size: self.buffer.len(),
            current_file_size: self.current_file_size,
            file_counter: self.file_counter,
        }
    }
}

/// 输出统计信息
#[derive(Debug)]
pub struct OutputStats {
    pub buffer_size: usize,
    pub current_file_size: u64,
    pub file_counter: u32,
}

/// 全局输出管理器包装器
pub struct GlobalOutputManager {
    manager: Arc<Mutex<OutputManager>>,
}

impl GlobalOutputManager {
    pub async fn new(config: OutputConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let manager = OutputManager::new(config).await?;
        Ok(Self {
            manager: Arc::new(Mutex::new(manager)),
        })
    }

    pub async fn save_result(&self, result: DetectionResult) -> Result<(), Box<dyn std::error::Error>> {
        self.manager.lock().await.save_result(result).await
    }

    pub async fn flush(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.manager.lock().await.flush().await
    }

    pub async fn stats(&self) -> OutputStats {
        self.manager.lock().await.stats()
    }
}

impl Clone for GlobalOutputManager {
    fn clone(&self) -> Self {
        Self {
            manager: Arc::clone(&self.manager),
        }
    }
}
