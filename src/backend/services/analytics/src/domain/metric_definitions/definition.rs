use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    HigherIsBetter,
    LowerIsBetter,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricFormat {
    Integer,
    Decimal,
    Currency,
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricComputation {
    Sum,
    Count,
    CountDistinct,
    Ratio,
    Distribution,
    Gauge,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricInputRole {
    Value,
    Event,
    Numerator,
    Denominator,
    Sample,
    Snapshot,
    Dependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionStatistic {
    P50,
    P75,
    P90,
    P95,
    P99,
    Avg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GaugeMethod {
    Latest,
    Min,
    Max,
    Avg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSource {
    AiMetricObservations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CohortSource {
    MetricEntityCohortsCurrent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetricDefinition {
    Sum(SumMetricDefinition),
    Count(CountMetricDefinition),
    CountDistinct(CountDistinctMetricDefinition),
    Ratio(RatioMetricDefinition),
    Distribution(DistributionMetricDefinition),
    Gauge(GaugeMetricDefinition),
    Derived(DerivedMetricDefinition),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricBase {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub explanation: Option<String>,
    pub entity_type: String,
    pub format: MetricFormat,
    pub unit: Option<String>,
    pub direction: MetricDirection,
    pub peer_cohort_key: Option<String>,
    pub allowed_dimensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricInput {
    pub role: MetricInputRole,
    pub observation_source: ObservationSource,
    pub source_key: String,
    pub measure_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SumMetricDefinition {
    pub base: MetricBase,
    pub value: MetricInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CountMetricDefinition {
    pub base: MetricBase,
    pub event: MetricInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CountDistinctMetricDefinition {
    pub base: MetricBase,
    pub event: MetricInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RatioMetricDefinition {
    pub base: MetricBase,
    pub numerator: MetricInput,
    pub denominator: MetricInput,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistributionMetricDefinition {
    pub base: MetricBase,
    pub sample: MetricInput,
    pub statistic: DistributionStatistic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GaugeMetricDefinition {
    pub base: MetricBase,
    pub snapshot: MetricInput,
    pub method: GaugeMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedMetricDefinition {
    pub base: MetricBase,
    pub dependencies: Vec<MetricInput>,
}

impl MetricDefinition {
    pub fn key(&self) -> &str {
        self.base().key.as_str()
    }

    pub fn base(&self) -> &MetricBase {
        match self {
            Self::Sum(def) => &def.base,
            Self::Count(def) => &def.base,
            Self::CountDistinct(def) => &def.base,
            Self::Ratio(def) => &def.base,
            Self::Distribution(def) => &def.base,
            Self::Gauge(def) => &def.base,
            Self::Derived(def) => &def.base,
        }
    }

    pub fn computation(&self) -> MetricComputation {
        match self {
            Self::Sum(_) => MetricComputation::Sum,
            Self::Count(_) => MetricComputation::Count,
            Self::CountDistinct(_) => MetricComputation::CountDistinct,
            Self::Ratio(_) => MetricComputation::Ratio,
            Self::Distribution(_) => MetricComputation::Distribution,
            Self::Gauge(_) => MetricComputation::Gauge,
            Self::Derived(_) => MetricComputation::Derived,
        }
    }

    pub fn allowed_dimension(&self, dimension: &str) -> Option<&str> {
        self.base()
            .allowed_dimensions
            .iter()
            .map(String::as_str)
            .find(|d| *d == dimension)
    }

    pub fn executable(&self) -> Option<ExecutableMetric> {
        match self {
            Self::Sum(def) => Some(ExecutableMetric::Sum(def.clone())),
            Self::Ratio(def) => Some(ExecutableMetric::Ratio(def.clone())),
            Self::Count(_)
            | Self::CountDistinct(_)
            | Self::Distribution(_)
            | Self::Gauge(_)
            | Self::Derived(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutableMetric {
    Sum(SumMetricDefinition),
    Ratio(RatioMetricDefinition),
}

impl ExecutableMetric {
    pub fn is_zero_filled(&self) -> bool {
        matches!(self, Self::Sum(_))
    }

    pub fn observation_source(&self) -> ObservationSource {
        match self {
            Self::Sum(def) => def.value.observation_source,
            Self::Ratio(def) => def.numerator.observation_source,
        }
    }
}

impl ObservationSource {
    pub fn from_ref(value: &str) -> Option<Self> {
        match value {
            "ai_metric_observations" => Some(Self::AiMetricObservations),
            _ => None,
        }
    }

    pub fn table_ref(self) -> (&'static str, &'static str) {
        match self {
            Self::AiMetricObservations => ("insight", "ai_metric_observations"),
        }
    }
}

impl CohortSource {
    pub fn table_ref(self) -> (&'static str, &'static str) {
        match self {
            Self::MetricEntityCohortsCurrent => ("insight", "metric_entity_cohorts_current"),
        }
    }
}

impl MetricFormat {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "integer" => Some(Self::Integer),
            "decimal" => Some(Self::Decimal),
            "currency" => Some(Self::Currency),
            "percent" => Some(Self::Percent),
            _ => None,
        }
    }
}

impl MetricDirection {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "higher_is_better" => Some(Self::HigherIsBetter),
            "lower_is_better" => Some(Self::LowerIsBetter),
            "neutral" => Some(Self::Neutral),
            _ => None,
        }
    }
}

impl MetricComputation {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "sum" => Some(Self::Sum),
            "count" => Some(Self::Count),
            "count_distinct" => Some(Self::CountDistinct),
            "ratio" => Some(Self::Ratio),
            "distribution" => Some(Self::Distribution),
            "gauge" => Some(Self::Gauge),
            "derived" => Some(Self::Derived),
            _ => None,
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Count => "count",
            Self::CountDistinct => "count_distinct",
            Self::Ratio => "ratio",
            Self::Distribution => "distribution",
            Self::Gauge => "gauge",
            Self::Derived => "derived",
        }
    }
}

impl MetricInputRole {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "value" => Some(Self::Value),
            "event" => Some(Self::Event),
            "numerator" => Some(Self::Numerator),
            "denominator" => Some(Self::Denominator),
            "sample" => Some(Self::Sample),
            "snapshot" => Some(Self::Snapshot),
            "dependency" => Some(Self::Dependency),
            _ => None,
        }
    }
}

impl DistributionStatistic {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "p50" => Some(Self::P50),
            "p75" => Some(Self::P75),
            "p90" => Some(Self::P90),
            "p95" => Some(Self::P95),
            "p99" => Some(Self::P99),
            "avg" => Some(Self::Avg),
            _ => None,
        }
    }
}

impl GaugeMethod {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "latest" => Some(Self::Latest),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "avg" => Some(Self::Avg),
            _ => None,
        }
    }
}
