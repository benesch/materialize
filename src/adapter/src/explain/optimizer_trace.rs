// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! Tracing utilities for explainable plans.

use std::fmt::{Debug, Display};
use std::time::Duration;

use mz_compute_types::dataflows::DataflowDescription;
use mz_compute_types::plan::Plan;
use mz_expr::explain::ExplainContext;
use mz_expr::{MirRelationExpr, MirScalarExpr, OptimizedMirRelationExpr, RowSetFinishing};
use mz_repr::explain::tracing::{DelegateSubscriber, PlanTrace, TraceEntry};
use mz_repr::explain::{
    Explain, ExplainConfig, ExplainError, ExplainFormat, ExprHumanizer, UsedIndexes,
};
use mz_sql::plan::{HirRelationExpr, HirScalarExpr};
use mz_sql_parser::ast::ExplainStage;
use mz_transform::dataflow::DataflowMetainfo;
use mz_transform::optimizer_notices::OptimizerNotice;
use tracing::dispatcher;
use tracing_subscriber::prelude::*;

use crate::coord::peek::FastPathPlan;
use crate::explain::Explainable;

/// Provides functionality for tracing plans generated by the execution of an
/// optimization pipeline.
///
/// Internally, this will create a layered [`tracing::subscriber::Subscriber`]
/// consisting of one layer for each supported plan type `T`.
///
/// Use `tracing::dispatcher::set_default` to trace in synchronous context.
/// Use `tracing::instrument::WithSubscriber::with_subscriber(&optimizer_trace)` to trace the result of a `Future`.
///
/// The [`OptimizerTrace::drain_all`] method on the created instance can be
/// then used to collect the trace, and [`OptimizerTrace::drain_all`] to obtain
/// the collected trace as a vector of [`TraceEntry`] instances.
pub(crate) struct OptimizerTrace(dispatcher::Dispatch);

impl OptimizerTrace {
    /// Create a new [`OptimizerTrace`].
    ///
    /// The instance will will only accumulate [`TraceEntry`] instances along
    /// the prefix of the given `path` if `path` is present, or it will
    /// accumulate all [`TraceEntry`] instances otherwise.
    pub fn new(broken: bool, path: Option<&'static str>) -> OptimizerTrace {
        if broken {
            let subscriber = DelegateSubscriber::default()
                // Collect `explain_plan` types that are not used in the regular explain
                // path, but are useful when instrumenting code for debugging purpuses.
                .with(PlanTrace::<String>::new(path))
                .with(PlanTrace::<HirScalarExpr>::new(path))
                .with(PlanTrace::<MirScalarExpr>::new(path))
                // Collect `explain_plan` types that are used in the regular explain path.
                .with(PlanTrace::<HirRelationExpr>::new(path))
                .with(PlanTrace::<MirRelationExpr>::new(path))
                .with(PlanTrace::<DataflowDescription<OptimizedMirRelationExpr>>::new(path))
                .with(PlanTrace::<DataflowDescription<Plan>>::new(path));

            OptimizerTrace(dispatcher::Dispatch::new(subscriber))
        } else {
            let subscriber = tracing_subscriber::registry()
                // Collect `explain_plan` types that are not used in the regular explain
                // path, but are useful when instrumenting code for debugging purpuses.
                .with(PlanTrace::<String>::new(path))
                .with(PlanTrace::<HirScalarExpr>::new(path))
                .with(PlanTrace::<MirScalarExpr>::new(path))
                // Collect `explain_plan` types that are used in the regular explain path.
                .with(PlanTrace::<HirRelationExpr>::new(path))
                .with(PlanTrace::<MirRelationExpr>::new(path))
                .with(PlanTrace::<DataflowDescription<OptimizedMirRelationExpr>>::new(path))
                .with(PlanTrace::<DataflowDescription<Plan>>::new(path));

            OptimizerTrace(dispatcher::Dispatch::new(subscriber))
        }
    }

    /// Collect all traced plans for all plan types `T` that are available in
    /// the wrapped [`dispatcher::Dispatch`].
    pub fn drain_all(
        self,
        format: ExplainFormat,
        config: &ExplainConfig,
        humanizer: &dyn ExprHumanizer,
        row_set_finishing: Option<RowSetFinishing>,
        used_indexes: UsedIndexes,
        fast_path_plan: Option<FastPathPlan>,
        dataflow_metainfo: DataflowMetainfo,
    ) -> Result<Vec<TraceEntry<String>>, ExplainError> {
        let mut results = vec![];

        // First, create an ExplainContext without `used_indexes`. We'll use this to, e.g., drain
        // HIR plans.
        let mut context = ExplainContext {
            config,
            humanizer,
            used_indexes: UsedIndexes::default(),
            finishing: row_set_finishing.clone(),
            duration: Duration::default(),
            optimizer_notices: OptimizerNotice::explain(
                &dataflow_metainfo.optimizer_notices,
                humanizer,
            )?,
        };

        // Drain trace entries of types produced by local optimizer stages.
        results.extend(itertools::chain!(
            self.drain_explainable_entries::<HirRelationExpr>(&format, &mut context, &None)?,
            self.drain_explainable_entries::<MirRelationExpr>(&format, &mut context, &None)?,
        ));

        // Drain trace entries of types produced by global optimizer stages.
        let mut context = ExplainContext {
            config,
            humanizer,
            used_indexes,
            finishing: row_set_finishing,
            duration: Duration::default(),
            optimizer_notices: OptimizerNotice::explain(
                &dataflow_metainfo.optimizer_notices,
                humanizer,
            )?,
        };
        let fast_path_plan = match fast_path_plan {
            Some(mut plan) if !context.config.no_fast_path => {
                Some(Explainable::new(&mut plan).explain(&format, &context)?)
            }
            _ => None,
        };
        results.extend(itertools::chain!(
            self.drain_explainable_entries::<DataflowDescription<OptimizedMirRelationExpr>>(
                &format,
                &mut context,
                &fast_path_plan
            )?,
            self.drain_explainable_entries::<DataflowDescription<Plan>>(
                &format,
                &mut context,
                &fast_path_plan
            )?,
        ));

        // Drain trace entries of type String, HirScalarExpr, MirScalarExpr
        // which are useful for ad-hoc debugging.
        results.extend(itertools::chain!(
            self.drain_scalar_entries::<HirScalarExpr>(),
            self.drain_scalar_entries::<MirScalarExpr>(),
            self.drain_string_entries(),
        ));

        // sort plans by instant (TODO: this can be implemented in a more
        // efficient way, as we can assume that each of the runs that are used
        // to `*.extend` the `results` vector is already sorted).
        results.sort_by_key(|x| x.instant);

        Ok(results)
    }

    /// Collect all trace entries of a plan type `T` that implements
    /// [`Explainable`].
    fn drain_explainable_entries<T>(
        &self,
        format: &ExplainFormat,
        context: &mut ExplainContext,
        fast_path_plan: &Option<String>,
    ) -> Result<Vec<TraceEntry<String>>, ExplainError>
    where
        T: Clone + Debug + 'static,
        for<'a> Explainable<'a, T>: Explain<'a, Context = ExplainContext<'a>>,
    {
        if let Some(trace) = self.0.downcast_ref::<PlanTrace<T>>() {
            trace
                .drain_as_vec()
                .into_iter()
                .map(|mut entry| {
                    // update the context with the current time
                    context.duration = entry.full_duration;
                    match fast_path_plan {
                        Some(fast_path_plan)
                            if !context.config.no_fast_path && {
                                [
                                    ExplainStage::OptimizedPlan.path().expect("path"),
                                    ExplainStage::PhysicalPlan.path().expect("path"),
                                ]
                                .contains(&entry.path.as_str())
                            } =>
                        {
                            Ok(TraceEntry {
                                instant: entry.instant,
                                span_duration: entry.span_duration,
                                full_duration: entry.full_duration,
                                path: entry.path,
                                plan: fast_path_plan.clone(),
                            })
                        }
                        _ => Ok(TraceEntry {
                            instant: entry.instant,
                            span_duration: entry.span_duration,
                            full_duration: entry.full_duration,
                            path: entry.path,
                            plan: Explainable::new(&mut entry.plan).explain(format, context)?,
                        }),
                    }
                })
                .collect()
        } else {
            unreachable!("drain_explainable_entries called with wrong plan type T");
        }
    }

    /// Collect all trace entries of a plan type `T`.
    fn drain_scalar_entries<T>(&self) -> Vec<TraceEntry<String>>
    where
        T: Clone + Debug + 'static,
        T: Display,
    {
        if let Some(trace) = self.0.downcast_ref::<PlanTrace<T>>() {
            trace
                .drain_as_vec()
                .into_iter()
                .map(|entry| TraceEntry {
                    instant: entry.instant,
                    span_duration: entry.span_duration,
                    full_duration: entry.full_duration,
                    path: entry.path,
                    plan: entry.plan.to_string(),
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Collect all trace entries with plans of type [`String`].
    fn drain_string_entries(&self) -> Vec<TraceEntry<String>> {
        if let Some(trace) = self.0.downcast_ref::<PlanTrace<String>>() {
            trace.drain_as_vec()
        } else {
            vec![]
        }
    }
}

impl From<&OptimizerTrace> for tracing::Dispatch {
    fn from(value: &OptimizerTrace) -> Self {
        // be not afraid: value.0 is a Dispatcher, which is Arc<dyn Subscriber + ...>
        // https://docs.rs/tracing-core/0.1.30/src/tracing_core/dispatcher.rs.html#451-453
        value.0.clone()
    }
}
