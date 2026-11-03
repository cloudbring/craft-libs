#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! A native, asynchronous `log` backend for desktop applications.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::VecDeque;
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
    use std::thread;
    use std::time::SystemTime;

    use log::{Level, LevelFilter, Log, Metadata, Record};

    pub const KEEP: usize = 2;
    pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
    pub const MAX_PENDING: usize = 512;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Filter {
        default: LevelFilter,
        directives: Vec<(String, LevelFilter)>,
    }

    impl Filter {
        pub fn parse(spec: &str) -> Self {
            let mut filter = Self { default: LevelFilter::Error, directives: Vec::new() };
            for part in spec.split(',').map(str::trim).filter(|part| !part.is_empty()) {
                if let Some((name, level)) = part.split_once('=') {
                    if !name.trim().is_empty()
                        && let Ok(level) = level.trim().parse()
                    {
                        filter.directives.push((name.trim().to_owned(), level));
                    }
                } else if let Ok(level) = part.parse() {
                    filter.default = level;
                } else {
                    filter.directives.push((part.to_owned(), LevelFilter::Trace));
                }
            }
            filter
        }

        pub fn level_for(&self, target: &str) -> LevelFilter {
            self.directives
                .iter()
                .filter(|(name, _)| {
                    name.strip_suffix('*').map_or_else(
                        || target.strip_prefix(name).is_some_and(|rest| rest.is_empty() || rest.starts_with("::")),
                        |prefix| target.starts_with(prefix),
                    )
                })
                .max_by_key(|(name, _)| name.len())
                .map_or(self.default, |(_, level)| *level)
        }
    }

    pub fn format_line(timestamp: &str, level: Level, thread: &str, target: &str, message: &str) -> String {
        let message = message.strip_suffix('\n').unwrap_or(message);
        format!("{timestamp} {level:<5} [{thread}] {target}: {message}\n")
    }

    struct State {
        queue: VecDeque<String>,
        recent: VecDeque<String>,
        active: bool,
        dropped: u64,
    }

    struct Shared {
        state: Mutex<State>,
        changed: Condvar,
        errors: AtomicU64,
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub struct Logger {
        filter: Mutex<Filter>,
        env_filter: Option<Filter>,
        shared: Arc<Shared>,
    }

    impl Logger {
        fn new(dir: PathBuf, stem: String, settings: &str, env: Option<String>, max_bytes: u64) -> io::Result<Self> {
            fs::create_dir_all(&dir)?;
            let file = open_log(&dir, &stem)?;
            let shared = Arc::new(Shared {
                state: Mutex::new(State { queue: VecDeque::new(), recent: VecDeque::new(), active: false, dropped: 0 }),
                changed: Condvar::new(),
                errors: AtomicU64::new(0),
            });
            let worker = Arc::clone(&shared);
            thread::Builder::new().name("craft-log".into()).spawn(move || writer(worker, dir, stem, file, max_bytes))?;
            Ok(Self {
                filter: Mutex::new(Filter::parse(settings)),
                env_filter: env.filter(|value| !value.trim().is_empty()).map(|value| Filter::parse(&value)),
                shared,
            })
        }

        pub fn set_filter(&self, spec: &str) {
            *lock(&self.filter) = Filter::parse(spec);
        }
        pub fn recent(&self) -> Vec<String> {
            lock(&self.shared.state).recent.iter().cloned().collect()
        }
        pub fn dropped(&self) -> u64 {
            lock(&self.shared.state).dropped
        }
        pub fn write_errors(&self) -> u64 {
            self.shared.errors.load(Ordering::Relaxed)
        }

        fn enqueue(&self, line: String) {
            let mut state = lock(&self.shared.state);
            if state.recent.len() == MAX_PENDING {
                state.recent.pop_front();
            }
            state.recent.push_back(line.clone());
            if state.queue.len() == MAX_PENDING {
                state.queue.pop_front();
                state.dropped = state.dropped.saturating_add(1);
            }
            state.queue.push_back(line);
            self.shared.changed.notify_one();
        }

        pub fn flush(&self) {
            let mut state = lock(&self.shared.state);
            while !state.queue.is_empty() || state.active {
                state = self.shared.changed.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
    }

    impl Log for Logger {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            let settings = lock(&self.filter);
            metadata.level() <= self.env_filter.as_ref().unwrap_or(&settings).level_for(metadata.target())
        }
        fn log(&self, record: &Record<'_>) {
            if self.enabled(record.metadata()) {
                let time: chrono::DateTime<chrono::Local> = SystemTime::now().into();
                let thread = thread::current();
                self.enqueue(format_line(
                    &time.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                    record.level(),
                    thread.name().unwrap_or("?"),
                    record.target(),
                    &record.args().to_string(),
                ));
            }
        }
        fn flush(&self) {
            Logger::flush(self);
        }
    }

    fn path(dir: &Path, stem: &str, index: usize) -> PathBuf {
        if index == 0 { dir.join(format!("{stem}.log")) } else { dir.join(format!("{stem}.{index}.log")) }
    }
    fn open_log(dir: &Path, stem: &str) -> io::Result<File> {
        OpenOptions::new().create(true).append(true).open(path(dir, stem, 0))
    }
    fn rotate(dir: &Path, stem: &str) -> io::Result<File> {
        for index in (1..KEEP).rev() {
            let destination = path(dir, stem, index);
            match fs::remove_file(&destination) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
            match fs::rename(path(dir, stem, index - 1), destination) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        open_log(dir, stem)
    }
    fn write_line(file: &mut Option<File>, dir: &Path, stem: &str, max_bytes: u64, line: &str) -> io::Result<()> {
        if file.is_none() {
            *file = Some(open_log(dir, stem)?);
        }
        let size = file.as_ref().ok_or_else(|| io::Error::other("log file unavailable"))?.metadata()?.len();
        if size > 0 && size.saturating_add(line.len() as u64) > max_bytes {
            drop(file.take());
            *file = Some(rotate(dir, stem)?);
        }
        let file = file.as_mut().ok_or_else(|| io::Error::other("log file unavailable"))?;
        file.write_all(line.as_bytes())?;
        file.sync_data()
    }
    fn writer(shared: Arc<Shared>, dir: PathBuf, stem: String, file: File, max_bytes: u64) {
        let mut file = Some(file);
        loop {
            let line = {
                let mut state = lock(&shared.state);
                while state.queue.is_empty() {
                    state = shared.changed.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                state.active = true;
                state.queue.pop_front()
            };
            if let Some(line) = line
                && write_line(&mut file, &dir, &stem, max_bytes, &line).is_err()
            {
                shared.errors.fetch_add(1, Ordering::Relaxed);
            }
            let mut state = lock(&shared.state);
            state.active = false;
            shared.changed.notify_all();
        }
    }

    static LOGGER: OnceLock<Logger> = OnceLock::new();
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    static INSTALL: Mutex<()> = Mutex::new(());

    /// Installs once. The environment variable is derived from `app`, e.g. `PHOTOCRAFT_LOG`.
    pub fn init(app: &str, dir: impl AsRef<Path>, settings: &str) -> Result<&'static Logger, String> {
        let _guard = lock(&INSTALL);
        if let Some(logger) = LOGGER.get() {
            return if INSTALLED.get() == Some(&true) { Ok(logger) } else { Err("another logger is installed".into()) };
        }
        if app.is_empty() || !app.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
            return Err("app must be a nonempty ASCII name".into());
        }
        let env_name = format!("{}_LOG", app.replace('-', "_").to_ascii_uppercase());
        let logger = Logger::new(dir.as_ref().to_owned(), app.to_ascii_lowercase(), settings, std::env::var(env_name).ok(), MAX_FILE_BYTES)
            .map_err(|error| error.to_string())?;
        let logger = LOGGER.get_or_init(|| logger);
        let installed = log::set_logger(logger).is_ok();
        let _ = INSTALLED.set(installed);
        if !installed {
            return Err("another logger is installed".into());
        }
        log::set_max_level(LevelFilter::Trace);
        Ok(logger)
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;
        use std::sync::atomic::AtomicUsize;

        fn dir() -> PathBuf {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            std::env::temp_dir().join(format!("craft-log-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
        }
        fn deckcraft_format_line(ts: &str, level: log::Level, thread: &str, target: &str, message: &str) -> String {
            let message = message.strip_suffix('\n').unwrap_or(message);
            format!("{ts} {level:<5} [{thread}] {target}: {message}\n")
        }
        fn numbered(dir: &Path, n: usize) -> PathBuf {
            if n == 0 { dir.join("deckcraft.log") } else { dir.join(format!("deckcraft.{n}.log")) }
        }
        fn deckcraft_rotate(dir: &Path) -> std::io::Result<(PathBuf, File)> {
            std::fs::create_dir_all(dir)?;
            for n in (1..=KEEP).rev() {
                match std::fs::rename(numbered(dir, n - 1), numbered(dir, n)) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
            let path = numbered(dir, 0);
            let file = File::create(&path)?;
            Ok((path, file))
        }
        #[test]
        fn differential_deckcraft_line_and_layout() {
            let directory = dir();
            let baseline = dir();
            fs::create_dir_all(&directory).expect("directory");
            fs::create_dir_all(&baseline).expect("baseline");
            let line = deckcraft_format_line("2026-01-01T00:00:00.000Z", Level::Info, "main", "deckcraft", "hello\n");
            assert_eq!(line, format_line("2026-01-01T00:00:00.000Z", Level::Info, "main", "deckcraft", "hello\n"));
            let mut file = Some(open_log(&directory, "deckcraft").expect("open"));
            write_line(&mut file, &directory, "deckcraft", 1000, &line).expect("write");
            assert_eq!(fs::read_to_string(directory.join("deckcraft.log")).expect("read"), line);
            let (_, mut original) = deckcraft_rotate(&baseline).expect("rotate");
            original.write_all(line.as_bytes()).expect("write");
            drop(original);
            assert_eq!(fs::read(baseline.join("deckcraft.log")).expect("baseline"), fs::read(directory.join("deckcraft.log")).expect("shared"));
            drop(file);
            fs::remove_dir_all(directory).expect("cleanup");
            fs::remove_dir_all(baseline).expect("cleanup");
        }
        #[test]
        fn rotation_keeps_two_ordered_files() {
            let directory = dir();
            fs::create_dir_all(&directory).expect("directory");
            let mut file = Some(open_log(&directory, "app").expect("open"));
            for line in ["1234", "5678", "abcd", "efgh", "ijkl"] {
                write_line(&mut file, &directory, "app", 8, line).expect("write");
            }
            assert_eq!(fs::read_to_string(path(&directory, "app", 0)).expect("current"), "ijkl");
            assert_eq!(fs::read_to_string(path(&directory, "app", 1)).expect("old"), "abcdefgh");
            assert_eq!(fs::read_dir(&directory).expect("list").count(), KEEP);
            drop(file);
            fs::remove_dir_all(directory).expect("cleanup");
        }
        #[test]
        fn filters_and_env_precedence() {
            let filter = Filter::parse("warn,app*=info,app::detail=debug");
            assert_eq!(filter.level_for("other"), LevelFilter::Warn);
            assert_eq!(filter.level_for("app_engine"), LevelFilter::Info);
            assert_eq!(filter.level_for("app::detail"), LevelFilter::Debug);
            let logger = Logger::new(dir(), "app".into(), "off", Some("debug".into()), 1024).expect("logger");
            logger.set_filter("off");
            assert!(logger.enabled(&Metadata::builder().level(Level::Debug).target("other").build()));
        }
        #[test]
        fn concurrent_queue_and_recent_ring() {
            let directory = dir();
            let logger = Arc::new(Logger::new(directory.clone(), "app".into(), "trace", None, 1_000_000).expect("logger"));
            let mut threads = Vec::new();
            for thread in 0..8 {
                let logger = Arc::clone(&logger);
                threads.push(std::thread::spawn(move || {
                    for number in 0..50 {
                        logger.enqueue(format!("{thread}:{number}\n"));
                    }
                }));
            }
            for thread in threads {
                thread.join().expect("join");
            }
            logger.flush();
            assert_eq!(logger.dropped(), 0);
            assert_eq!(logger.recent().len(), 400);
            assert_eq!(logger.write_errors(), 0);
            let disk = fs::read_to_string(path(&directory, "app", 0)).expect("disk");
            assert_eq!(disk.lines().count(), 400);
            for thread in 0..8 {
                for number in 0..50 {
                    assert!(disk.lines().any(|line| line == format!("{thread}:{number}")));
                }
            }
            for number in 0..600 {
                logger.enqueue(format!("last:{number}\n"));
            }
            let recent = logger.recent();
            assert_eq!(recent.len(), MAX_PENDING);
            assert_eq!(recent.first().expect("first"), "last:88\n");
            assert_eq!(recent.last().expect("last"), "last:599\n");
            logger.flush();
        }
        #[test]
        fn concurrent_overflow_counts_oldest_drops() {
            let logger = Arc::new(Logger {
                filter: Mutex::new(Filter::parse("trace")),
                env_filter: None,
                shared: Arc::new(Shared {
                    state: Mutex::new(State { queue: VecDeque::new(), recent: VecDeque::new(), active: false, dropped: 0 }),
                    changed: Condvar::new(),
                    errors: AtomicU64::new(0),
                }),
            });
            let mut threads = Vec::new();
            for thread in 0..8 {
                let logger = Arc::clone(&logger);
                threads.push(std::thread::spawn(move || {
                    for number in 0..100 {
                        logger.enqueue(format!("{thread}:{number}"));
                    }
                }));
            }
            for thread in threads {
                thread.join().expect("join");
            }
            let state = lock(&logger.shared.state);
            assert_eq!(state.dropped, 800 - MAX_PENDING as u64);
            assert_eq!(state.queue, state.recent);
        }
        #[test]
        fn init_twice() {
            let first = init("craft_log_test", dir(), "info").expect("init");
            let second = init("ignored", dir(), "off").expect("second init");
            assert!(std::ptr::eq(first, second));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
