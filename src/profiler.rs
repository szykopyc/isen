use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{Error, Result};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CURRENT: RefCell<Option<Rc<RefCell<Profile>>>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    category: String,
    name: String,
    source: String,
    line: usize,
}

#[derive(Clone, Debug, Default)]
struct Metric {
    calls: u64,
    total_ns: u128,
    self_ns: u128,
}

#[derive(Clone, Debug)]
struct Active {
    key: Key,
    started: Instant,
    child_ns: u128,
    edge: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct Profile {
    programme: PathBuf,
    started: Instant,
    elapsed_ns: u128,
    success: bool,
    resources_started: Option<Resources>,
    user_cpu_ns: u128,
    system_cpu_ns: u128,
    peak_rss_bytes: u64,
    spans: BTreeMap<Key, Metric>,
    edges: BTreeMap<(String, String), Metric>,
    counters: BTreeMap<String, u64>,
    active: Vec<Active>,
}

impl Profile {
    fn new(programme: PathBuf) -> Self {
        Self {
            programme,
            started: Instant::now(),
            elapsed_ns: 0,
            success: false,
            resources_started: resources(),
            user_cpu_ns: 0,
            system_cpu_ns: 0,
            peak_rss_bytes: 0,
            spans: BTreeMap::new(),
            edges: BTreeMap::new(),
            counters: BTreeMap::new(),
            active: Vec::new(),
        }
    }

    pub(crate) fn human(&self) -> String {
        let mut output = String::new();
        let _ = writeln!(output, "\nIsen profile: {}", self.programme.display());
        let _ = writeln!(
            output,
            "status: {}    wall: {}",
            if self.success { "success" } else { "failure" },
            duration(self.elapsed_ns)
        );
        if self.peak_rss_bytes > 0 {
            let cpu_percent = if self.elapsed_ns == 0 {
                0.0
            } else {
                (self.user_cpu_ns + self.system_cpu_ns) as f64 / self.elapsed_ns as f64 * 100.0
            };
            let _ = writeln!(
                output,
                "cpu: user {}    system {}    utilisation {:.1}%    peak RSS {}",
                duration(self.user_cpu_ns),
                duration(self.system_cpu_ns),
                cpu_percent,
                bytes(self.peak_rss_bytes)
            );
        }

        self.write_category(&mut output, "phase", "Phases", usize::MAX);
        self.write_category(&mut output, "function", "Isen functions", 15);
        self.write_category(&mut output, "native", "Native calls", 15);
        self.write_lines(&mut output, 20);
        self.write_aggregated_category(&mut output, "statement", "Statement kinds", 15);
        self.write_aggregated_category(&mut output, "expression", "Expression kinds", 15);
        self.write_edges(&mut output, 15);

        if !self.counters.is_empty() {
            let _ = writeln!(output, "\nRuntime counters");
            for (name, value) in &self.counters {
                let _ = writeln!(output, "  {name:<30} {value:>12}");
            }
        }
        output
    }

    pub(crate) fn json(&self) -> String {
        let mut output = String::new();
        let _ = write!(
            output,
            "{{\n  \"format\": \"isen-profile-v1\",\n  \"programme\": \"{}\",\n  \"success\": {},\n  \"wall_ns\": {},\n  \"user_cpu_ns\": {},\n  \"system_cpu_ns\": {},\n  \"peak_rss_bytes\": {},\n",
            escape(&self.programme.display().to_string()),
            self.success,
            self.elapsed_ns,
            self.user_cpu_ns,
            self.system_cpu_ns,
            self.peak_rss_bytes
        );
        output.push_str("  \"counters\": {");
        for (index, (name, value)) in self.counters.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            let _ = write!(output, "\n    \"{}\": {}", escape(name), value);
        }
        if !self.counters.is_empty() {
            output.push('\n');
        }
        output.push_str("  },\n  \"spans\": [");
        for (index, (key, metric)) in self.spans.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            let _ = write!(
                output,
                "\n    {{\"category\": \"{}\", \"name\": \"{}\", \"source\": \"{}\", \"line\": {}, \"calls\": {}, \"total_ns\": {}, \"self_ns\": {}}}",
                escape(&key.category),
                escape(&key.name),
                escape(&key.source),
                key.line,
                metric.calls,
                metric.total_ns,
                metric.self_ns
            );
        }
        if !self.spans.is_empty() {
            output.push('\n');
        }
        output.push_str("  ],\n  \"call_edges\": [");
        for (index, ((caller, callee), metric)) in self.edges.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            let _ = write!(
                output,
                "\n    {{\"caller\": \"{}\", \"callee\": \"{}\", \"calls\": {}, \"total_ns\": {}}}",
                escape(caller),
                escape(callee),
                metric.calls,
                metric.total_ns
            );
        }
        if !self.edges.is_empty() {
            output.push('\n');
        }
        output.push_str("  ]\n}\n");
        output
    }

    pub(crate) fn write_json(&self, path: &Path) -> Result<()> {
        fs::write(path, self.json())
            .map_err(|error| Error::new(0, error.to_string()).with_source(path))
    }

    fn write_category(&self, output: &mut String, category: &str, title: &str, limit: usize) {
        let mut rows = self
            .spans
            .iter()
            .filter(|(key, _)| key.category == category)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return;
        }
        rows.sort_by_key(|(_, metric)| std::cmp::Reverse(metric.total_ns));
        let _ = writeln!(output, "\n{title}");
        let _ = writeln!(
            output,
            "  {:<30} {:>10} {:>12} {:>12}",
            "name", "calls", "total", "self"
        );
        for (key, metric) in rows.into_iter().take(limit) {
            let label = if key.source.is_empty() {
                key.name.clone()
            } else if key.line == 0 {
                format!("{} ({})", key.name, short_source(&key.source))
            } else {
                format!("{} ({}:{})", key.name, short_source(&key.source), key.line)
            };
            let _ = writeln!(
                output,
                "  {:<30} {:>10} {:>12} {:>12}",
                truncate(&label, 30),
                metric.calls,
                duration(metric.total_ns),
                duration(metric.self_ns)
            );
        }
    }

    fn write_aggregated_category(
        &self,
        output: &mut String,
        category: &str,
        title: &str,
        limit: usize,
    ) {
        let mut aggregated: BTreeMap<&str, Metric> = BTreeMap::new();
        for (key, metric) in self
            .spans
            .iter()
            .filter(|(key, _)| key.category == category)
        {
            let combined = aggregated.entry(&key.name).or_default();
            combined.calls += metric.calls;
            combined.total_ns += metric.total_ns;
            combined.self_ns += metric.self_ns;
        }
        if aggregated.is_empty() {
            return;
        }
        let mut rows = aggregated.into_iter().collect::<Vec<_>>();
        rows.sort_by_key(|(_, metric)| std::cmp::Reverse(metric.total_ns));
        let _ = writeln!(output, "\n{title}");
        let _ = writeln!(
            output,
            "  {:<30} {:>10} {:>12} {:>12}",
            "name", "calls", "total", "self"
        );
        for (name, metric) in rows.into_iter().take(limit) {
            let _ = writeln!(
                output,
                "  {:<30} {:>10} {:>12} {:>12}",
                truncate(name, 30),
                metric.calls,
                duration(metric.total_ns),
                duration(metric.self_ns)
            );
        }
    }

    fn write_lines(&self, output: &mut String, limit: usize) {
        let mut lines: BTreeMap<(String, usize), Metric> = BTreeMap::new();
        for (key, metric) in self
            .spans
            .iter()
            .filter(|(key, _)| key.category == "statement")
        {
            let line = lines.entry((key.source.clone(), key.line)).or_default();
            line.calls += metric.calls;
            line.total_ns += metric.total_ns;
            line.self_ns += metric.self_ns;
        }
        if lines.is_empty() {
            return;
        }
        let mut rows = lines.into_iter().collect::<Vec<_>>();
        rows.sort_by_key(|(_, metric)| std::cmp::Reverse(metric.total_ns));
        let _ = writeln!(output, "\nHot source lines");
        let _ = writeln!(
            output,
            "  {:<38} {:>10} {:>12} {:>12}",
            "location", "hits", "total", "self"
        );
        for ((source, line), metric) in rows.into_iter().take(limit) {
            let location = format!("{}:{line}", short_source(&source));
            let _ = writeln!(
                output,
                "  {:<38} {:>10} {:>12} {:>12}",
                truncate(&location, 38),
                metric.calls,
                duration(metric.total_ns),
                duration(metric.self_ns)
            );
        }
    }

    fn write_edges(&self, output: &mut String, limit: usize) {
        if self.edges.is_empty() {
            return;
        }
        let mut rows = self.edges.iter().collect::<Vec<_>>();
        rows.sort_by_key(|(_, metric)| std::cmp::Reverse(metric.total_ns));
        let _ = writeln!(output, "\nHot call edges");
        let _ = writeln!(
            output,
            "  {:<38} {:>10} {:>12}",
            "caller → callee", "calls", "total"
        );
        for ((caller, callee), metric) in rows.into_iter().take(limit) {
            let edge = format!("{caller} -> {callee}");
            let _ = writeln!(
                output,
                "  {:<38} {:>10} {:>12}",
                truncate(&edge, 38),
                metric.calls,
                duration(metric.total_ns)
            );
        }
    }
}

pub(crate) fn start(programme: &Path) {
    CURRENT.with(|current| {
        *current.borrow_mut() = Some(Rc::new(RefCell::new(Profile::new(programme.to_owned()))));
    });
    ACTIVE.with(|active| active.set(true));
}

pub(crate) fn active() -> bool {
    ACTIVE.with(Cell::get)
}

pub(crate) fn finish(success: bool) -> Profile {
    let profile = CURRENT.with(|current| current.borrow_mut().take());
    ACTIVE.with(|active| active.set(false));
    let profile = profile.expect("profiler must be started before it is finished");
    let mut profile = Rc::try_unwrap(profile)
        .unwrap_or_else(|_| panic!("profiler still has active owners"))
        .into_inner();
    profile.elapsed_ns = profile.started.elapsed().as_nanos();
    if let (Some(started), Some(finished)) = (profile.resources_started, resources()) {
        profile.user_cpu_ns = finished.user_cpu_ns.saturating_sub(started.user_cpu_ns);
        profile.system_cpu_ns = finished.system_cpu_ns.saturating_sub(started.system_cpu_ns);
        profile.peak_rss_bytes = finished.peak_rss_bytes;
    }
    profile.success = success;
    profile
}

pub(crate) fn span<T>(
    category: &str,
    name: &str,
    source: Option<&Path>,
    line: usize,
    operation: impl FnOnce() -> T,
) -> T {
    if !active() {
        return operation();
    }
    begin(category, name, source, line);
    let result = operation();
    end();
    result
}

pub(crate) fn count(name: &str, amount: u64) {
    if !active() {
        return;
    }
    CURRENT.with(|current| {
        let Some(profile) = current.borrow().as_ref().cloned() else {
            return;
        };
        let mut profile = profile.borrow_mut();
        *profile.counters.entry(name.to_owned()).or_default() += amount;
    });
}

pub(crate) fn maximum(name: &str, value: u64) {
    if !active() {
        return;
    }
    CURRENT.with(|current| {
        let Some(profile) = current.borrow().as_ref().cloned() else {
            return;
        };
        let mut profile = profile.borrow_mut();
        let counter = profile.counters.entry(name.to_owned()).or_default();
        *counter = (*counter).max(value);
    });
}

fn begin(category: &str, name: &str, source: Option<&Path>, line: usize) {
    CURRENT.with(|current| {
        let profile = current.borrow().as_ref().cloned().unwrap();
        let mut profile = profile.borrow_mut();
        let qualified = if let Some(source) = source {
            format!(
                "{}:{}:{line}",
                short_source(&source.display().to_string()),
                name
            )
        } else {
            name.to_owned()
        };
        let edge = if matches!(category, "function" | "native") {
            let caller = profile
                .active
                .iter()
                .rev()
                .find(|active| matches!(active.key.category.as_str(), "function" | "native"))
                .map(|active| qualified_key(&active.key))
                .unwrap_or_else(|| "<programme>".into());
            Some((caller, qualified))
        } else {
            None
        };
        profile.active.push(Active {
            key: Key {
                category: category.to_owned(),
                name: name.to_owned(),
                source: source
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                line,
            },
            started: Instant::now(),
            child_ns: 0,
            edge,
        });
        let span_depth = profile.active.len() as u64;
        maximum_locked(&mut profile, "maximum_span_depth", span_depth);
        if matches!(category, "function" | "native") {
            let depth = profile
                .active
                .iter()
                .filter(|active| matches!(active.key.category.as_str(), "function" | "native"))
                .count() as u64;
            maximum_locked(&mut profile, "maximum_call_depth", depth);
        }
    });
}

fn end() {
    CURRENT.with(|current| {
        let profile = current.borrow().as_ref().cloned().unwrap();
        let mut profile = profile.borrow_mut();
        let active = profile
            .active
            .pop()
            .expect("profile spans must be balanced");
        let elapsed = active.started.elapsed().as_nanos();
        let metric = profile.spans.entry(active.key).or_default();
        metric.calls += 1;
        metric.total_ns += elapsed;
        metric.self_ns += elapsed.saturating_sub(active.child_ns);
        if let Some(parent) = profile.active.last_mut() {
            parent.child_ns += elapsed;
        }
        if let Some(edge) = active.edge {
            let metric = profile.edges.entry(edge).or_default();
            metric.calls += 1;
            metric.total_ns += elapsed;
        }
    });
}

fn maximum_locked(profile: &mut Profile, name: &str, value: u64) {
    let counter = profile.counters.entry(name.to_owned()).or_default();
    *counter = (*counter).max(value);
}

fn duration(nanoseconds: u128) -> String {
    let duration = Duration::from_nanos(nanoseconds.min(u64::MAX as u128) as u64);
    if duration.as_secs() >= 1 {
        format!("{:.3}s", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{:.3}ms", duration.as_secs_f64() * 1_000.0)
    } else if duration.as_micros() >= 1 {
        format!("{:.3}us", duration.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{}ns", duration.as_nanos())
    }
}

fn bytes(value: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = value as f64;
    if value >= GIB {
        format!("{:.2} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.2} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.2} KiB", value / KIB)
    } else {
        format!("{value:.0} B")
    }
}

#[derive(Clone, Copy, Debug)]
struct Resources {
    user_cpu_ns: u128,
    system_cpu_ns: u128,
    peak_rss_bytes: u64,
}

#[cfg(unix)]
fn resources() -> Option<Resources> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the provided rusage structure on success,
    // and the value is only assumed initialized after a zero return code.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: guarded by the successful getrusage result above.
    let usage = unsafe { usage.assume_init() };
    let timeval_ns = |time: libc::timeval| {
        (time.tv_sec.max(0) as u128) * 1_000_000_000 + (time.tv_usec.max(0) as u128) * 1_000
    };
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let peak_rss_bytes = usage.ru_maxrss.max(0) as u64;
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let peak_rss_bytes = (usage.ru_maxrss.max(0) as u64).saturating_mul(1024);
    Some(Resources {
        user_cpu_ns: timeval_ns(usage.ru_utime),
        system_cpu_ns: timeval_ns(usage.ru_stime),
        peak_rss_bytes,
    })
}

#[cfg(not(unix))]
fn resources() -> Option<Resources> {
    None
}

fn short_source(source: &str) -> &str {
    source.rsplit('/').next().unwrap_or(source)
}

fn qualified_key(key: &Key) -> String {
    if key.source.is_empty() {
        key.name.clone()
    } else {
        format!("{}:{}:{}", short_source(&key.source), key.name, key.line)
    }
}

fn truncate(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        return value.to_owned();
    }
    let mut output = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute;

    #[test]
    fn captures_language_runtime_activity_and_json() {
        start(Path::new("profile-test.is"));
        let result = execute(
            r#"
            given twice(value @@ int) @@ int $ ret value * 2 \$
            dec total @@ int = 0
            each value in [1, 2, 3] $
              total = total + twice(value)
            \$
            "#,
        );
        let profile = finish(result.is_ok());
        result.unwrap();

        let human = profile.human();
        assert!(human.contains("Isen functions"));
        assert!(human.contains("twice"));
        assert!(human.contains("each_iterations"));

        let json = profile.json();
        assert!(json.contains("\"format\": \"isen-profile-v1\""));
        assert!(json.contains("\"category\": \"function\""));
        assert!(json.contains("\"name\": \"twice\""));
    }

    #[test]
    fn finishes_a_profile_after_a_programme_failure() {
        start(Path::new("failing-profile-test.is"));
        let result = execute("scream(\"measured failure\")");
        let profile = finish(result.is_ok());
        assert!(result.is_err());
        assert!(profile.human().contains("status: failure"));
        assert!(!active());
    }

    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(escape("a\n\"b\\c"), "a\\n\\\"b\\\\c");
    }
}
