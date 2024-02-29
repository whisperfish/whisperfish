use diesel::connection::InstrumentationEvent;

#[derive(Default)]
pub struct Instrumentation {
    query_log: std::collections::HashMap<String, u32>,
}

impl Instrumentation {
    fn query_to_key(query: &str) -> &str {
        query.split(" --").next().unwrap_or(query)
    }
}

impl diesel::connection::Instrumentation for Instrumentation {
    fn on_connection_event(&mut self, ev: InstrumentationEvent<'_>) {
        if let InstrumentationEvent::StartQuery { query, .. } = ev {
            self.query_log
                .entry(Self::query_to_key(&query.to_string()).to_string())
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }
    }
}

impl Drop for Instrumentation {
    fn drop(&mut self) {
        let mut query_log = self
            .query_log
            .drain()
            .map(|(query, count)| (count, query))
            .collect::<Vec<_>>();
        query_log.sort_unstable_by_key(|(count, _)| *count);
        for (count, query) in query_log {
            println!("Query: {query} was executed {count} times");
        }
    }
}
