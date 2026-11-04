use crate::report::{FileResult, Policy, Report};
use crate::{Judge, Outcome, Registry, Side, SideId};
use craft_compare::Verdict;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// What one side did on one input.
enum Ran<O> {
    Ok(O),
    Err(String),
    Panic(String),
    Timeout,
}

/// Runs `old` and `new` over inputs and judges the outputs.
///
/// Each side runs in its own worker thread per file. A side that exceeds the timeout is
/// abandoned (the thread cannot be killed and keeps running until it returns), so a hanging
/// implementation leaks one thread per hung file.
pub struct Differential<Old, New, J> {
    old: Arc<Old>,
    new: Arc<New>,
    judge: J,
    timeout: Duration,
    registry: Registry,
    policy: Policy,
    artifact_dir: Option<PathBuf>,
}

impl<Old, New, J> Differential<Old, New, J>
where
    Old: Side + Send + Sync + 'static,
    New: Side<Output = Old::Output> + Send + Sync + 'static,
    Old::Output: Send + 'static,
    J: Judge<Old::Output>,
{
    pub fn new(old: Old, new: New, judge: J) -> Self {
        Self {
            old: Arc::new(old),
            new: Arc::new(new),
            judge,
            timeout: Duration::from_secs(30),
            registry: Registry::default(),
            policy: Policy::default(),
            artifact_dir: None,
        }
    }

    /// Per-side, per-file timeout (default 30 s).
    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Registered expected differences.
    #[must_use]
    pub fn expected(mut self, r: Registry) -> Self {
        self.registry = r;
        self
    }

    /// Policy deciding which inputs are saved as artefacts.
    #[must_use]
    pub fn policy(mut self, p: Policy) -> Self {
        self.policy = p;
        self
    }

    /// Save every failing input (per the policy) and its outcome JSON here.
    #[must_use]
    pub fn artifact_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.artifact_dir = Some(dir.into());
        self
    }

    /// Run every `(name, bytes)` pair. Names are what `glob` in the registry matches.
    pub fn run_files<I, N, B>(&self, files: I) -> Report
    where
        I: IntoIterator<Item = (N, B)>,
        N: Into<String>,
        B: AsRef<[u8]>,
    {
        let mut hits = vec![false; self.registry.entries().len()];
        let mut results = Vec::new();
        for (i, (name, bytes)) in files.into_iter().enumerate() {
            let (name, bytes) = (name.into(), bytes.as_ref());
            let mut outcome = self.run_one(bytes);
            if let Some(idx) = self.registry.find(&name, &outcome) {
                hits[idx] = true;
                outcome = Outcome::ExpectedDifference { id: self.registry.entries()[idx].id.clone() };
            }
            if self.policy.is_failure(&outcome) {
                self.save(i, &name, bytes, &outcome);
            }
            results.push(FileResult { name, outcome });
        }
        let stale = self.registry.entries().iter().zip(&hits).filter(|(_, h)| !**h).map(|(e, _)| e.id.clone()).collect();
        Report::new(results, stale)
    }

    /// Run every file of `corpus` carrying `tag`; names are corpus-relative paths.
    pub fn run_corpus(&self, corpus: &craft_corpus::Corpus, tag: &str) -> std::io::Result<Report> {
        let mut files = Vec::new();
        for f in corpus.files_by_tag(tag) {
            let bytes = std::fs::read(&f.path).map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", f.path.display())))?;
            files.push((f.rel, bytes));
        }
        Ok(self.run_files(files))
    }

    fn run_one(&self, input: &[u8]) -> Outcome {
        let shared: Arc<[u8]> = Arc::from(input);
        let deadline = Instant::now() + self.timeout;
        // Start both sides before waiting on either, so they run concurrently.
        let rx_old = spawn(&self.old, &shared);
        let rx_new = spawn(&self.new, &shared);
        let old = collect(&rx_old, deadline);
        let new = collect(&rx_new, deadline);
        match (old, new) {
            (Ran::Panic(msg), _) => Outcome::Panic { side: SideId::Old, msg },
            (_, Ran::Panic(msg)) => Outcome::Panic { side: SideId::New, msg },
            (Ran::Timeout, _) => Outcome::Timeout { side: SideId::Old },
            (_, Ran::Timeout) => Outcome::Timeout { side: SideId::New },
            (Ran::Err(old_err), Ran::Err(new_err)) => Outcome::BothFail { old_err, new_err },
            (Ran::Err(old_err), Ran::Ok(_)) => Outcome::OnlyOldFails { old_err },
            (Ran::Ok(_), Ran::Err(new_err)) => Outcome::OnlyNewFails { new_err },
            (Ran::Ok(o), Ran::Ok(n)) => match self.judge.judge(&o, &n) {
                Verdict::Identical => Outcome::Identical,
                Verdict::Within { metric, value } => Outcome::WithinTolerance { metric, value },
                Verdict::Exceeds { metric, value, limit } => Outcome::Regression { metric, value, limit },
            },
        }
    }

    fn save(&self, idx: usize, name: &str, bytes: &[u8], outcome: &Outcome) {
        let Some(dir) = &self.artifact_dir else { return };
        let stem = format!("{idx:05}-{}", sanitize(name));
        // Best effort: an unwritable artefact dir must not hide the verdict itself.
        if std::fs::create_dir_all(dir).is_ok() {
            let _ = std::fs::write(Path::new(dir).join(format!("{stem}.bin")), bytes);
            let meta = serde_json::json!({ "name": name, "outcome": outcome });
            let _ = std::fs::write(Path::new(dir).join(format!("{stem}.json")), serde_json::to_string_pretty(&meta).unwrap_or_default());
        }
    }
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
}

type Msg<O> = Result<Result<O, String>, String>;

fn spawn<S>(side: &Arc<S>, input: &Arc<[u8]>) -> mpsc::Receiver<Msg<S::Output>>
where
    S: Side + Send + Sync + 'static,
    S::Output: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let (side, input) = (Arc::clone(side), Arc::clone(input));
    let spawned = std::thread::Builder::new().name("craft-oracle-side".into()).spawn({
        let tx = tx.clone();
        move || {
            let r = catch_unwind(AssertUnwindSafe(|| side.run(&input))).map_err(|p| panic_message(p.as_ref()));
            let _ = tx.send(r);
        }
    });
    if let Err(e) = spawned {
        let _ = tx.send(Ok(Err(format!("could not spawn worker thread: {e}"))));
    }
    rx
}

fn collect<O>(rx: &mpsc::Receiver<Msg<O>>, deadline: Instant) -> Ran<O> {
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(Ok(o))) => Ran::Ok(o),
        Ok(Ok(Err(e))) => Ran::Err(e),
        Ok(Err(msg)) => Ran::Panic(msg),
        Err(mpsc::RecvTimeoutError::Timeout) => Ran::Timeout,
        Err(mpsc::RecvTimeoutError::Disconnected) => Ran::Panic("worker thread ended without a result".into()),
    }
}

fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BytesJudge, FnSide};

    type R = Result<Vec<u8>, String>;

    fn run<O: Fn(&[u8]) -> R + Send + Sync + 'static, N: Fn(&[u8]) -> R + Send + Sync + 'static>(o: O, n: N, ms: u64) -> Outcome {
        let d = Differential::new(FnSide(o), FnSide(n), BytesJudge).timeout(Duration::from_millis(ms));
        d.run_files([("f", vec![1u8, 2, 3])]).results.remove(0).outcome
    }

    #[test]
    fn identical_and_regression() {
        assert_eq!(run(|b| Ok(b.to_vec()), |b| Ok(b.to_vec()), 1000), Outcome::Identical);
        assert!(matches!(run(|b| Ok(b.to_vec()), |_| Ok(vec![9]), 1000), Outcome::Regression { .. }));
    }

    #[test]
    fn failure_combinations() {
        assert!(matches!(run(|_| Err("a".into()), |_| Err("b".into()), 1000), Outcome::BothFail { .. }));
        assert!(matches!(run(|_| Err("a".into()), |b| Ok(b.to_vec()), 1000), Outcome::OnlyOldFails { .. }));
        assert!(matches!(run(|b| Ok(b.to_vec()), |_| Err("b".into()), 1000), Outcome::OnlyNewFails { .. }));
    }

    #[test]
    fn panic_is_caught_per_side() {
        let o = run(|b| Ok(b.to_vec()), |_| panic!("boom"), 1000);
        assert_eq!(o, Outcome::Panic { side: SideId::New, msg: "boom".into() });
        let o = run(|_| -> R { panic!("{}", 7) }, |b| Ok(b.to_vec()), 1000);
        assert_eq!(o, Outcome::Panic { side: SideId::Old, msg: "7".into() });
    }

    #[test]
    fn hang_times_out() {
        let o = run(
            |b| Ok(b.to_vec()),
            |b| {
                std::thread::sleep(Duration::from_millis(600));
                Ok(b.to_vec())
            },
            100,
        );
        assert_eq!(o, Outcome::Timeout { side: SideId::New });
    }

    #[test]
    fn registry_turns_regression_into_expected_and_reports_stale() {
        let reg = Registry::from_toml_str(
            "[[expected]]\nid='fix'\nreason='r'\nevidence='e'\nglob='a/*'\n[[expected]]\nid='unused'\nreason='r'\nevidence='e'\nglob='zzz'\n",
        )
        .unwrap();
        let d = Differential::new(FnSide(|b: &[u8]| Ok(b.to_vec())), FnSide(|_: &[u8]| Ok(vec![0u8])), BytesJudge).expected(reg);
        let r = d.run_files([("a/1", [5u8]), ("b/1", [5u8])]);
        assert_eq!(r.results[0].outcome, Outcome::ExpectedDifference { id: "fix".into() });
        assert!(matches!(r.results[1].outcome, Outcome::Regression { .. }));
        assert_eq!(r.stale, vec!["unused".to_string()]);
    }
}
