use rand::{Rng, RngExt};
use rand_distr::Normal;
use serde::{Deserialize, Serialize};

// ============================================================
// DOCTRINE
// ============================================================

/// Engagement doctrine for a defender platform.
///
/// - `ShootLookShoot`: Fire one interceptor per threat, assess the result,
///   then proceed to the next threat or defender.
/// - `MaxDefense`: Commit all available engagement capacity against a single
///   threat simultaneously before the next defender acts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Doctrine {
    #[serde(rename = "shoot-look-shoot")]
    ShootLookShoot,

    #[serde(rename = "max_defense")]
    MaxDefense,
}

impl Doctrine {
    /// Human-readable label for display in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Doctrine::ShootLookShoot => "Shoot-Look-Shoot",
            Doctrine::MaxDefense => "Max Defense",
        }
    }
}

// ============================================================
// CONFIGURATION MODELS  (deserialized from TOML)
// ============================================================

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ScenarioConfig {
    pub scenario: ScenarioMeta,
    #[serde(default)]
    pub defenders: Vec<Defender>,
    #[serde(default)]
    pub attackers: Vec<Attacker>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ScenarioMeta {
    pub name: String,
    /// This field is read by TOML deserialization and Tera templates only.
    #[allow(dead_code)]
    pub description: Option<String>,
    pub iterations: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Defender {
    pub name: String,
    pub staying_power: u32,
    pub magazine_depth: u32,
    pub max_engagement_capacity: u32,
    pub doctrine: Doctrine,
    pub pkd: Probability,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Attacker {
    pub name: String,
    pub target_name: String,
    pub salvo_schedule: Vec<u32>,
    pub pko: Probability,
}

// ============================================================
// PROBABILITY  (resolved stochastically per run)
// ============================================================

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum Probability {
    #[serde(rename = "fixed")]
    Fixed { value: f64 },

    #[serde(rename = "uniform_range")]
    Uniform { min: f64, max: f64 },

    #[serde(rename = "normal_distribution")]
    Normal { mean: f64, std_dev: f64 },
}

impl Probability {
    /// Draw a single value from this probability distribution.
    pub fn to_value(&self, rng: &mut impl Rng) -> f64 {
        match self {
            Probability::Fixed { value } => *value,
            Probability::Uniform { min, max } => rng.random_range(*min..=*max),
            Probability::Normal { mean, std_dev } => {
                let normal = Normal::new(*mean, *std_dev).unwrap();
                rng.sample(normal).clamp(0.0, 1.0)
            }
        }
    }

    /// Human-readable label for display in the UI.
    pub fn label(&self) -> String {
        match self {
            Probability::Fixed { value } => format!("{:.0}%", value * 100.0),
            Probability::Uniform { min, max } => {
                format!("Uniform({:.0}–{:.0}%)", min * 100.0, max * 100.0)
            }
            Probability::Normal { mean, std_dev } => {
                format!("Normal(μ={:.0}%, σ={:.0}%)", mean * 100.0, std_dev * 100.0)
            }
        }
    }
}

// ============================================================
// RUNTIME STATE  (mutable per-iteration)
// ============================================================

#[derive(Clone, Debug, Serialize)]
pub struct RunDefender {
    pub name: String,
    pub staying_power: u32,
    pub hits_taken: u32,
    pub magazine_depth: u32,
    pub max_engagement_capacity: u32,
    pub pkd: f64,
    pub doctrine: Doctrine,
}

impl From<&Defender> for RunDefender {
    /// Structural copy — `pkd` is set to `0.0` and must be resolved
    /// separately via `Probability::to_value()` for each iteration.
    fn from(d: &Defender) -> Self {
        RunDefender {
            name: d.name.clone(),
            staying_power: d.staying_power,
            hits_taken: 0,
            magazine_depth: d.magazine_depth,
            max_engagement_capacity: d.max_engagement_capacity,
            pkd: 0.0, // placeholder; resolved per-iteration
            doctrine: d.doctrine.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct IncomingThreat {
    pub target_name: String,
    pub pko: f64,
}

// ============================================================
// AGGREGATION  (accumulated across iterations)
// ============================================================

#[derive(Default)]
pub struct DefenderStats {
    pub survival_count: u32,
    pub total_hits_taken: u32,
    pub total_interceptors_spent: u32,
}

// ============================================================
// OUTPUT  (returned by the engine, safe for export)
// ============================================================

/// Top-level result of a full Monte Carlo simulation.
#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub scenario_name: String,
    pub iterations: u32,
    pub defenders: Vec<DefenderResult>,
    pub attackers: Vec<AttackerResult>,
}

/// Per-defender summary computed from aggregated stats.
#[derive(Debug, Serialize)]
pub struct DefenderResult {
    pub name: String,
    pub staying_power: u32,
    pub magazine_depth: u32,
    pub survival_count: u32,
    pub survival_rate: f64,
    pub avg_hits_taken: f64,
    pub avg_interceptors_fired: f64,
}

/// Per-attacker summary for display in the results page.
#[derive(Debug, Serialize)]
pub struct AttackerResult {
    pub name: String,
    pub target_name: String,
    pub total_missiles: u32,
    pub pko_label: String,
}

// ============================================================
// STEP-THROUGH / INTERACTIVE EVENT TYPES
// ============================================================

/// One attacker's salvo during a single time step.
#[derive(Debug, Clone, Serialize)]
pub struct ThreatLaunch {
    pub attacker_name: String,
    pub target_name: String,
    pub missiles_fired: u32,
    pub pko_label: String,
}

/// Interception activity by a single defender during one step.
#[derive(Debug, Clone, Serialize)]
pub struct InterceptionEvent {
    pub defender_name: String,
    pub doctrine_label: String,
    pub interceptors_fired: u32,
    pub threats_intercepted: u32,
    pub max_engagement_capacity: u32,
}

/// Damage impact from surviving threats against a single target.
#[derive(Debug, Clone, Serialize)]
pub struct ImpactEvent {
    pub target_name: String,
    pub missiles_leaked: u32,
    pub hits_scored: u32,
    pub hp_before: u32,
    pub hp_after: u32,
    pub destroyed: bool,
}

/// A full snapshot of one time step in an interactive simulation.
#[derive(Debug, Clone, Serialize)]
pub struct StepRecord {
    pub step: usize,
    pub max_steps: usize,
    pub is_complete: bool,
    pub threat_launches: Vec<ThreatLaunch>,
    pub interceptions: Vec<InterceptionEvent>,
    pub impacts: Vec<ImpactEvent>,
    pub defender_snapshots: Vec<RunDefender>,
}
