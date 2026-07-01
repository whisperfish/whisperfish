//! Diesel connection instrumentation.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use diesel::connection::InstrumentationEvent;

/// How many of the most recent latencies to keep per query for the running statistics.
const RECENT_WINDOW: usize = 256;

#[derive(Default)]
pub struct Instrumentation {
    /// Per-query running statistics, keyed by the (bind-free) normalized SQL.
    stats: HashMap<String, QueryStats>,
    /// LIFO of start instants for `StartQuery`/`FinishQuery` pairing.
    start_stack: Vec<Instant>,
}

/// Running statistics for a single (normalized) query shape.
struct QueryStats {
    count: u32,
    total: Duration,
    min: Duration,
    max: Duration,
    /// Ring buffer of recent latencies for the percentile comparison.
    ///
    /// Cfr. [`RECENT_WINDOW`]
    recent: VecDeque<Duration>,
}

impl Default for QueryStats {
    fn default() -> Self {
        Self {
            count: 0,
            total: Duration::ZERO,
            min: Duration::MAX,
            max: Duration::ZERO,
            recent: VecDeque::new(),
        }
    }
}

impl QueryStats {
    /// Record a successful run. The percentile/median helpers read `recent`
    /// *before* calling this, so they compare against prior runs only.
    fn record(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total = self.total.saturating_add(elapsed);
        if elapsed < self.min {
            self.min = elapsed;
        }
        if elapsed > self.max {
            self.max = elapsed;
        }
        if self.recent.len() == RECENT_WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(elapsed);
    }

    /// Fraction (0..=100) of prior recent runs that were at most `elapsed`.
    /// `None` until there is a baseline. High percentile == this run was slow.
    fn percentile_against_prior(&self, elapsed: Duration) -> Option<f64> {
        let n = self.recent.len();
        if n == 0 {
            return None;
        }
        let at_most = self.recent.iter().filter(|&&s| s <= elapsed).count();
        Some(at_most as f64 / n as f64 * 100.0)
    }

    /// Median of the prior recent runs, for a "typical" reference point.
    fn median_prior(&self) -> Option<Duration> {
        let mut v: Vec<Duration> = self.recent.iter().copied().collect();
        if v.is_empty() {
            return None;
        }
        v.sort_unstable();
        Some(v[v.len() / 2])
    }

    /// Mean latency over all recorded successful runs.
    fn mean(&self) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }

        self.total / self.count
    }
}

impl diesel::connection::Instrumentation for Instrumentation {
    fn on_connection_event(&mut self, ev: InstrumentationEvent<'_>) {
        match ev {
            InstrumentationEvent::StartQuery { .. } => {
                self.start_stack.push(Instant::now());
            }
            InstrumentationEvent::FinishQuery { query, error, .. } => {
                let started = match self.start_stack.pop() {
                    Some(t) => t,
                    None => return, // defensive: Finish without a matching Start
                };
                let elapsed = started.elapsed();

                // Strip the `-- binds: [...]` comment: the key is parameterized
                // SQL with no bound values.
                let rendered = query.to_string();
                let key = normalize_key(&rendered).to_owned();

                if let Some(err) = error {
                    // Don't fold failures into the "normal" latency baseline,
                    // but still surface them with their own timing.
                    tracing::trace!(
                        target: "whisperfish_store::db::query",
                        query = %key,
                        elapsed = ?elapsed,
                        "{key} — {elapsed:?} FAILED (excluded from baseline): {err}",
                    );
                    return;
                }

                // Compare this run against prior runs, then fold it in.
                let percentile = self
                    .stats
                    .get(&key)
                    .and_then(|s| s.percentile_against_prior(elapsed));
                let median = self.stats.get(&key).and_then(|s| s.median_prior());

                let stats = self.stats.entry(key.clone()).or_default();
                stats.record(elapsed);
                let mean = stats.mean();

                let pct = percentile
                    .map(|p| format!("p{p:.0}"))
                    .unwrap_or_else(|| "p—".to_string());
                let p50 = median
                    .map(|m| format!("{m:?}"))
                    .unwrap_or_else(|| "—".to_string());

                tracing::trace!(
                    target: "whisperfish_store::db::query",
                    query = %key,
                    elapsed = ?elapsed,
                    "{key} — {elapsed:?} (run #{}, min {:?}, p50 {p50}, mean {:?}, max {:?}) — {pct} of prior runs",
                    stats.count, stats.min, mean, stats.max,
                );
            }
            _ => {}
        }
    }
}

/// Reduce a rendered query to its bind-free key by dropping everything from the
/// first SQL comment (` --`) onwards, which is where Diesel appends
/// `-- binds: [...]`.
fn normalize_key(query: &str) -> &str {
    query.split(" --").next().unwrap_or(query)
}

impl Drop for Instrumentation {
    fn drop(&mut self) {
        let mut entries: Vec<_> = self
            .stats
            .drain()
            .map(|(query, s)| (s.total, s.count, s.min, s.max, s.mean(), query))
            .collect();
        // Cumulative time descending: the "energy" ranking.
        entries.sort_by(|(t_a, _, _, _, _, _), (t_b, _, _, _, _, _)| t_b.cmp(t_a));

        tracing::info!(
            target: "whisperfish_store::db::query",
            "Diesel query instrumentation summary (sorted by cumulative time):"
        );
        for (total, count, min, max, mean, query) in entries {
            tracing::info!(
                target: "whisperfish_store::db::query",
                count = count,
                total = ?total,
                "{query} — {count}× (min {min:?}, mean {mean:?}, max {max:?}, {total:?} cumulative)",
            );
        }
    }
}
