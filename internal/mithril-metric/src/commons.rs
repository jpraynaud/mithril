use prometheus::{core::Collector, Counter, Gauge, Opts};
use slog::{debug, Logger};

use mithril_common::{entities::Epoch, StdResult};

/// Type alias for a metric name.
pub type MetricName = str;

/// Type alias for a counter value.
pub type CounterValue = u32;

/// Mithril metric
pub trait MithrilMetric {
    /// Metric name
    fn name(&self) -> String;

    /// Wrapped prometheus collector
    fn collector(&self) -> Box<dyn Collector>;
}

pub struct MetricCounter {
    name: String,
    logger: Logger,
    counter: Box<Counter>,
}

impl MetricCounter {
    pub fn new(logger: Logger, name: &str, help: &str) -> StdResult<Self> {
        let counter = MetricCounter::create_metric_counter(name, help)?;
        Ok(Self {
            logger,
            name: name.to_string(),
            counter: Box::new(counter),
        })
    }

    pub fn record(&self) {
        debug!(self.logger, "incrementing '{}' counter", self.name);
        self.counter.inc();
    }

    pub fn get(&self) -> CounterValue {
        self.counter.get().round() as CounterValue
    }

    fn create_metric_counter(name: &MetricName, help: &str) -> StdResult<Counter> {
        let counter_opts = Opts::new(name, help);
        let counter = Counter::with_opts(counter_opts)?;

        Ok(counter)
    }
}

impl MithrilMetric for MetricCounter {
    fn collector(&self) -> Box<dyn Collector> {
        self.counter.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}

pub struct MetricGauge {
    name: String,
    logger: Logger,
    gauge: Box<Gauge>,
}

impl MetricGauge {
    pub fn new(logger: Logger, name: &str, help: &str) -> StdResult<Self> {
        let gauge = MetricGauge::create_metric_gauge(name, help)?;
        Ok(Self {
            logger,
            name: name.to_string(),
            gauge: Box::new(gauge),
        })
    }

    pub fn record(&self, epoch: Epoch) {
        debug!(
            self.logger,
            "set '{}' gauge value to {}", self.name, epoch.0
        );
        self.gauge.set(epoch.0 as f64);
    }

    pub fn get(&self) -> Epoch {
        Epoch(self.gauge.get().round() as u64)
    }

    fn create_metric_gauge(name: &MetricName, help: &str) -> StdResult<Gauge> {
        let gauge_opts = Opts::new(name, help);
        let gauge = Gauge::with_opts(gauge_opts)?;

        Ok(gauge)
    }
}
impl MithrilMetric for MetricGauge {
    fn collector(&self) -> Box<dyn Collector> {
        self.gauge.clone()
    }
    fn name(&self) -> String {
        self.name.clone()
    }
}

pub mod metrics_tools {

    use mithril_common::StdResult;
    use prometheus::TextEncoder;

    pub fn export_metrics(registry: &prometheus::Registry) -> StdResult<String> {
        // let mut buffer = vec![];
        let encoder = TextEncoder::new();
        let metric_families = registry.gather();
        // encoder.encode(&metric_families, &mut buffer)?;

        // Ok(String::from_utf8(buffer)?)

        let mut buffer = String::new();
        encoder.encode_utf8(&metric_families, &mut buffer)?;
        Ok(buffer)
    }
}

#[cfg(test)]
pub mod test_tools {
    use std::{io, sync::Arc};

    use slog::{Drain, Logger};
    use slog_async::Async;
    use slog_term::{CompactFormat, PlainDecorator};
    pub struct TestLogger;

    impl TestLogger {
        fn from_writer<W: io::Write + Send + 'static>(writer: W) -> Logger {
            let decorator = PlainDecorator::new(writer);
            let drain = CompactFormat::new(decorator).build().fuse();
            let drain = Async::new(drain).build().fuse();
            Logger::root(Arc::new(drain), slog::o!())
        }

        pub fn stdout() -> Logger {
            Self::from_writer(slog_term::TestStdoutWriter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_tools::TestLogger;

    #[test]
    fn test_metric_counter_can_be_incremented() {
        let metric =
            MetricCounter::new(TestLogger::stdout(), "test_counter", "test counter help").unwrap();
        assert_eq!(metric.name(), "test_counter");
        assert_eq!(metric.get(), 0);

        metric.record();
        assert_eq!(metric.get(), 1);
    }

    #[test]
    fn test_metric_gauge_can_be_set() {
        let metric =
            MetricGauge::new(TestLogger::stdout(), "test_gauge", "test gauge help").unwrap();
        assert_eq!(metric.name(), "test_gauge");
        assert_eq!(metric.get(), Epoch(0));

        metric.record(Epoch(12));
        assert_eq!(metric.get(), Epoch(12));
    }

    mod tools {
        use super::*;
        use prometheus::Registry;
        use prometheus_parse::Value;
        use std::{collections::BTreeMap, sync::Arc};

        use mithril_common::entities::Epoch;

        fn parse_metrics(raw_metrics: &str) -> StdResult<BTreeMap<String, Value>> {
            Ok(
                prometheus_parse::Scrape::parse(raw_metrics.lines().map(|s| Ok(s.to_owned())))?
                    .samples
                    .into_iter()
                    .map(|s| (s.metric, s.value))
                    .collect::<BTreeMap<_, _>>(),
            )
        }

        #[test]
        fn test_export_metrics() {
            let counter_metric =
                MetricCounter::new(TestLogger::stdout(), "test_counter", "test counter help")
                    .unwrap();
            counter_metric.record();

            let gauge_metric =
                MetricGauge::new(TestLogger::stdout(), "test_gauge", "test gauge help").unwrap();
            gauge_metric.record(Epoch(12));

            let registry = Registry::new();
            registry.register(counter_metric.collector());
            registry.register(gauge_metric.collector());

            let exported_metrics = metrics_tools::export_metrics(&registry).unwrap();

            let parsed_metrics = parse_metrics(&exported_metrics).unwrap();

            let parsed_metrics_expected = BTreeMap::from([
                (counter_metric.name(), Value::Counter(1.0)),
                (gauge_metric.name(), Value::Gauge(12.0)),
            ]);

            assert_eq!(parsed_metrics_expected, parsed_metrics);
        }
    }
}
