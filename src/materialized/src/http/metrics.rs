// Copyright Materialize, Inc. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! Metrics HTTP endpoints.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::Future;
use std::time::Instant;

use askama::Template;
use futures::future;
use hyper::{Body, Request, Response};
use prometheus::proto::MetricFamily;
use prometheus::{
    Encoder, IntGauge, IntGaugeVec,
    TextEncoder,
};

use metrics::{metric, MetricRegistry};

use crate::http::{util, Server};

pub struct Metrics {
    request_metrics_gather: IntGauge,
    request_metrics_encode: IntGauge,
}

impl Metrics {
    pub fn register_into(registry: &MetricRegistry) -> Metrics {
        let request_metrics: IntGaugeVec = registry.register(metric!(
            name: "mz_server_scrape_metrics_times",
            help: "how long it took to gather metrics",
            var_labels: ["action"],
        ));
        Metrics {
            request_metrics_gather: request_metrics.with_label_values(&["gather"]),
            request_metrics_encode: request_metrics.with_label_values(&["encode"]),
        }
    }
}

#[derive(Template)]
#[template(path = "http/templates/status.html")]
struct StatusTemplate<'a> {
    version: &'a str,
    query_count: u64,
    start_time: Instant,
    metrics: Vec<&'a PromMetric<'a>>,
}

impl Server {
    pub fn handle_prometheus(
        &self,
        _: Request<Body>,
    ) -> impl Future<Output = anyhow::Result<Response<Body>>> {
        let metric_families = self.gather_metrics();
        let mut buffer = Vec::new();
        let start = Instant::now();
        TextEncoder::new()
            .encode(&metric_families, &mut buffer)
            .expect("writing to vec cannot fail");
        self.metrics
            .request_metrics_encode
            .set(start.elapsed().as_micros() as i64);
        future::ok(Response::new(Body::from(buffer)))
    }

    pub fn handle_status(
        &self,
        _: Request<Body>,
    ) -> impl Future<Output = anyhow::Result<Response<Body>>> {
        let metric_families = self.gather_metrics();

        let desired_metrics = {
            let mut s = BTreeSet::new();
            s.insert("mz_dataflow_events_read_total");
            s.insert("mz_bytes_read_total");
            s.insert("mz_worker_command_queue_size");
            s.insert("mz_command_durations");
            s
        };

        let mut metrics = BTreeMap::new();
        for metric in &metric_families {
            let converted = PromMetric::from_metric_family(metric);
            match converted {
                Ok(m) => {
                    for m in m {
                        match m {
                            PromMetric::Counter { name, .. } => {
                                if desired_metrics.contains(name) {
                                    metrics.insert(name.to_string(), m);
                                }
                            }
                            PromMetric::Gauge { name, .. } => {
                                if desired_metrics.contains(name) {
                                    metrics.insert(name.to_string(), m);
                                }
                            }
                            PromMetric::Histogram {
                                name, ref labels, ..
                            } => {
                                if desired_metrics.contains(name) {
                                    metrics.insert(
                                        format!(
                                            "{}:{}",
                                            name,
                                            labels.get("command").unwrap_or(&"")
                                        ),
                                        m,
                                    );
                                }
                            }
                        }
                    }
                }
                Err(_) => continue,
            };
        }
        let mut query_count = metrics
            .get("mz_command_durations:query")
            .map(|m| {
                if let PromMetric::Histogram { count, .. } = m {
                    *count
                } else {
                    0
                }
            })
            .unwrap_or(0);
        query_count += metrics
            .get("mz_command_durations:execute")
            .map(|m| {
                if let PromMetric::Histogram { count, .. } = m {
                    *count
                } else {
                    0
                }
            })
            .unwrap_or(0);

        future::ok(util::template_response(StatusTemplate {
            version: crate::VERSION,
            query_count,
            start_time: self.start_time,
            metrics: metrics.values().collect(),
        }))
    }

    fn gather_metrics(&self) -> Vec<MetricFamily> {
        let start = Instant::now();
        let metrics = prometheus::gather();
        self.metrics
            .request_metrics_gather
            .set(start.elapsed().as_micros() as i64);
        metrics
    }
}

#[derive(Debug)]
enum PromMetric<'a> {
    Counter {
        name: &'a str,
        value: f64,
        labels: BTreeMap<&'a str, &'a str>,
    },
    Gauge {
        name: &'a str,
        value: f64,
        labels: BTreeMap<&'a str, &'a str>,
    },
    Histogram {
        name: &'a str,
        count: u64,
        labels: BTreeMap<&'a str, &'a str>,
    },
}

impl fmt::Display for PromMetric<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn fmt(
            f: &mut fmt::Formatter,
            name: &str,
            value: impl fmt::Display,
            labels: &BTreeMap<&str, &str>,
        ) -> fmt::Result {
            write!(f, "{} ", name)?;
            for (n, v) in labels.iter() {
                write!(f, "{}={} ", n, v)?;
            }
            writeln!(f, "{}", value)
        }
        match self {
            PromMetric::Counter {
                name,
                value,
                labels,
            } => fmt(f, name, value, labels),
            PromMetric::Gauge {
                name,
                value,
                labels,
            } => fmt(f, name, value, labels),
            PromMetric::Histogram {
                name,
                count,
                labels,
            } => fmt(f, name, count, labels),
        }
    }
}

impl PromMetric<'_> {
    fn from_metric_family<'a>(
        m: &'a prometheus::proto::MetricFamily,
    ) -> Result<Vec<PromMetric<'a>>, ()> {
        use prometheus::proto::MetricType;
        fn l2m(metric: &prometheus::proto::Metric) -> BTreeMap<&str, &str> {
            metric
                .get_label()
                .iter()
                .map(|lp| (lp.get_name(), lp.get_value()))
                .collect()
        }
        m.get_metric()
            .iter()
            .map(|metric| {
                Ok(match m.get_field_type() {
                    MetricType::COUNTER => PromMetric::Counter {
                        name: m.get_name(),
                        value: metric.get_counter().get_value(),
                        labels: l2m(metric),
                    },
                    MetricType::GAUGE => PromMetric::Gauge {
                        name: m.get_name(),
                        value: metric.get_gauge().get_value(),
                        labels: l2m(metric),
                    },
                    MetricType::HISTOGRAM => PromMetric::Histogram {
                        name: m.get_name(),
                        count: metric.get_histogram().get_sample_count(),
                        labels: l2m(metric),
                    },
                    _ => return Err(()),
                })
            })
            .collect()
    }
}
