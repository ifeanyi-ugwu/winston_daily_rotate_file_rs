use chrono::{DateTime, Local, Utc};
use logform::{Format, LogInfo};
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use winston_transport::Transport;

pub struct DailyRotateFileOptions {
    pub level: Option<String>,
    pub format: Option<Format>,
    pub filename: PathBuf,
    pub date_pattern: String,
    pub max_files: Option<u32>,
    pub max_size: Option<u64>, // in bytes
    pub dirname: Option<PathBuf>,
    pub zipped_archive: bool,
    pub utc: bool,
}

pub struct DailyRotateFile {
    file: Mutex<BufWriter<File>>,
    options: DailyRotateFileOptions,
    last_rotation: Mutex<DateTime<Utc>>,
}

impl DailyRotateFile {
    pub fn new(options: DailyRotateFileOptions) -> Self {
        let current_date = if options.utc {
            Utc::now()
        } else {
            Local::now().with_timezone(&Utc)
        };

        let file =
            Self::create_file(&options, &current_date).expect("Failed to create initial log file");

        DailyRotateFile {
            file: Mutex::new(BufWriter::new(file)),
            options,
            last_rotation: Mutex::new(current_date),
        }
    }

    fn create_file(
        options: &DailyRotateFileOptions,
        date: &DateTime<Utc>,
    ) -> std::io::Result<std::fs::File> {
        let filename =
            Self::get_filename(&options.filename, date, &options.date_pattern, options.utc);

        //let cwd = std::env::current_dir().unwrap();
        let default_dir = ".";
        let log_dir = options
            .dirname
            .as_deref()
            .unwrap_or_else(|| Path::new(default_dir));
        let full_path = log_dir.join(filename);

        //println!("Final log filename: {:?}", full_path);

        let parent = full_path.parent().unwrap_or(&log_dir);
        //println!("Creating directory: {:?}", parent);
        create_dir_all(parent)?;

        OpenOptions::new().create(true).append(true).open(full_path)
    }

    fn get_filename(base_path: &Path, date: &DateTime<Utc>, pattern: &str, utc: bool) -> PathBuf {
        let date_str = if utc {
            date.format(pattern).to_string()
        } else {
            date.with_timezone(&Local).format(pattern).to_string()
        };

        let mut filename = base_path.to_path_buf();
        let original_filename = filename
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("log");

        filename.set_file_name(format!("{}.{}", original_filename, date_str));
        filename
    }

    fn should_rotate(&self) -> bool {
        let now = Utc::now();

        let now_str = if self.options.utc {
            now.format(&self.options.date_pattern).to_string()
        } else {
            now.with_timezone(&Local)
                .format(&self.options.date_pattern)
                .to_string()
        };

        let last_rotation = self.last_rotation.lock().unwrap();
        let last_rotation_str = if self.options.utc {
            last_rotation.format(&self.options.date_pattern).to_string()
        } else {
            last_rotation
                .with_timezone(&Local)
                .format(&self.options.date_pattern)
                .to_string()
        };

        last_rotation_str != now_str
    }

    fn rotate(&self) {
        let now = Utc::now();
        let new_file = Self::create_file(&self.options, &now).expect("Failed to rotate log file");

        let mut file_lock = self.file.lock().unwrap();
        *file_lock = BufWriter::new(new_file);

        let mut last_rotation = self.last_rotation.lock().unwrap();
        *last_rotation = now;
    }

    pub fn builder() -> DailyRotateFileBuilder {
        DailyRotateFileBuilder::new()
    }
}

impl Transport for DailyRotateFile {
    fn log(&self, info: LogInfo) {
        if self.should_rotate() {
            self.rotate();
        }

        let mut file = match self.file.lock() {
            Ok(f) => f,
            Err(_) => {
                eprintln!("Failed to acquire file lock");
                return;
            }
        };

        if let Err(e) = writeln!(file, "{}", info.message) {
            eprintln!("Failed to write log: {}", e);
            return;
        }
    }

    fn flush(&self) -> Result<(), String> {
        let mut file = self.file.lock().unwrap();
        file.flush().map_err(|e| format!("Failed to flush: {}", e))
    }

    fn get_level(&self) -> Option<&String> {
        self.options.level.as_ref()
    }

    fn get_format(&self) -> Option<&Format> {
        self.options.format.as_ref()
    }
}

pub struct DailyRotateFileBuilder {
    level: Option<String>,
    format: Option<Format>,
    filename: Option<PathBuf>,
    date_pattern: String,
    max_files: Option<u32>,
    max_size: Option<u64>,
    dirname: Option<PathBuf>,
    zipped_archive: bool,
    utc: bool,
}

impl DailyRotateFileBuilder {
    pub fn new() -> Self {
        Self {
            level: None,
            format: None,
            filename: None,
            date_pattern: String::from("%Y-%m-%d"),
            max_files: None,
            max_size: None,
            dirname: None,
            zipped_archive: false,
            utc: false,
        }
    }

    pub fn level<T: Into<String>>(mut self, level: T) -> Self {
        self.level = Some(level.into());
        self
    }

    pub fn format(mut self, format: Format) -> Self {
        self.format = Some(format);
        self
    }

    pub fn filename<T: Into<PathBuf>>(mut self, filename: T) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn date_pattern<T: Into<String>>(mut self, pattern: T) -> Self {
        self.date_pattern = pattern.into();
        self
    }

    pub fn max_files(mut self, count: u32) -> Self {
        self.max_files = Some(count);
        self
    }

    pub fn max_size(mut self, size: u64) -> Self {
        self.max_size = Some(size);
        self
    }

    pub fn dirname<T: Into<PathBuf>>(mut self, dirname: T) -> Self {
        self.dirname = Some(dirname.into());
        self
    }

    pub fn zipped_archive(mut self, zipped: bool) -> Self {
        self.zipped_archive = zipped;
        self
    }

    pub fn utc(mut self, utc: bool) -> Self {
        self.utc = utc;
        self
    }

    pub fn build(self) -> Result<DailyRotateFile, String> {
        let filename = self.filename.ok_or("Filename is required")?;

        let options = DailyRotateFileOptions {
            level: self.level,
            format: self.format,
            filename,
            date_pattern: self.date_pattern,
            max_files: self.max_files,
            max_size: self.max_size,
            dirname: self.dirname,
            zipped_archive: self.zipped_archive,
            utc: self.utc,
        };

        Ok(DailyRotateFile::new(options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::fs;
    use tempfile::TempDir;

    fn setup_temp_dir() -> TempDir {
        let project_root = std::env::current_dir().expect("Failed to get current directory");
        TempDir::new_in(&project_root).expect("Failed to create temp directory in project folder")
    }

    fn create_test_transport(temp_dir: &TempDir) -> DailyRotateFile {
        let log_path = temp_dir.path().join("test.log");
        DailyRotateFile::builder()
            .filename(&log_path)
            .date_pattern("%Y-%m-%d")
            .max_files(3)
            .max_size(1024) // 1KB
            .build()
            .expect("Failed to create transport")
    }

    #[test]
    fn test_basic_logging() {
        let temp_dir = setup_temp_dir();
        let transport = create_test_transport(&temp_dir);

        let log_info = LogInfo {
            level: "info".to_string(),
            message: "Test message".to_string(),
            meta: Default::default(),
        };

        transport.log(log_info);
        transport.flush().expect("Failed to flush");

        // Check if log file exists and contains the message
        let date_str = Local::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("test.log.{}", date_str));
        let contents = fs::read_to_string(log_file).expect("Failed to read log file");
        assert!(contents.contains("Test message"));
    }
}
