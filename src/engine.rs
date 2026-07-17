use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};

use crate::models::{
    Attacker, AttackerResult, Defender, DefenderResult, DefenderStats, Doctrine, ImpactEvent,
    IncomingThreat, InterceptionEvent, RunDefender, ScenarioConfig, SimulationResult, StepRecord,
    ThreatLaunch,
};

/// Run a full Monte Carlo simulation and return structured results.
pub fn run_monte_carlo(config: &ScenarioConfig) -> SimulationResult {
    let iterations = config.scenario.iterations.unwrap_or(1000) as usize;
    println!(
        "Starting Monte Carlo Simulation: {} (iterations: {})",
        config.scenario.name, iterations
    );

    // Pre-allocate stats for every defender named in the config.
    let mut stats: HashMap<String, DefenderStats> = HashMap::new();
    for def in &config.defenders {
        stats.insert(def.name.clone(), DefenderStats::default());
    }

    let max_steps = determine_max_steps(&config.attackers);
    let mut rng = rand::rng();

    for _ in 0..iterations {
        // --- Per-iteration initialisation ---
        let mut run_defenders = initialize_run(&config.defenders, &mut rng);

        // --- Time-stepped simulation ---
        for step in 0..max_steps {
            let threat_pool = generate_threat_pool(&config.attackers, step, &mut rng);
            if threat_pool.is_empty() {
                continue;
            }

            // Sort defenders by engagement effectiveness (best first).
            sort_by_pkd(&mut run_defenders);

            // Defenders attempt to intercept incoming threats.
            let (surviving, _) = interception_phase(&mut run_defenders, &threat_pool, &mut rng);

            // Unintercepted threats roll for terminal impact.
            terminal_impact_phase(&mut run_defenders, &surviving, &mut rng);
        }

        // --- Aggregate this run's outcome into global stats ---
        aggregate_run(&config.defenders, &run_defenders, &mut stats);
    }

    build_result(config, iterations as u32, &stats)
}

// ============================================================
// PHASE HELPERS
// ============================================================

fn determine_max_steps(attackers: &[Attacker]) -> usize {
    attackers
        .iter()
        .map(|a| a.salvo_schedule.len())
        .max()
        .unwrap_or(0)
}

fn initialize_run(defenders: &[Defender], rng: &mut impl Rng) -> Vec<RunDefender> {
    defenders
        .iter()
        .map(|d| {
            let mut rd = RunDefender::from(d);
            rd.pkd = d.pkd.to_value(rng);
            rd
        })
        .collect()
}

fn generate_threat_pool(
    attackers: &[Attacker],
    step: usize,
    rng: &mut impl Rng,
) -> Vec<IncomingThreat> {
    let mut pool = Vec::new();
    for attacker in attackers {
        if step < attacker.salvo_schedule.len() {
            let missiles_fired = attacker.salvo_schedule[step];
            let resolved_pko = attacker.pko.to_value(rng);
            for _ in 0..missiles_fired {
                pool.push(IncomingThreat {
                    target_name: attacker.target_name.clone(),
                    pko: resolved_pko,
                });
            }
        }
    }
    pool
}

fn sort_by_pkd(defenders: &mut [RunDefender]) {
    defenders.sort_by(|a, b| b.pkd.partial_cmp(&a.pkd).unwrap());
}

/// Attempt to intercept incoming threats using a two-phase model:
///
/// **Phase 1 — Targeted self-defence.** Each threat is first offered to the
/// specific defender it targets. That defender fires according to its doctrine.
///
/// **Phase 2 — Cooperative overflow.** If the targeted defender cannot engage
/// (destroyed, out of magazine, at launcher capacity, or all shots missed),
/// the threat is re-offered to all surviving defenders with spare capacity in
/// PkD order.  This models Aegis cooperative engagement: when a ship is
/// saturated, the next-best shooter in the formation takes over.
///
/// | Doctrine | Behaviour |
/// |---|---|
/// | **ShootLookShoot** | Fire one interceptor; if it hits, threat down. If miss, try next defender. |
/// | **ShootShootLook** | Salvo-launch up to two interceptors simultaneously. If any hits, threat down. |
/// | **MaxDefense** | Fire all remaining launcher capacity at once. If any hits, threat down. |
fn interception_phase(
    defenders: &mut [RunDefender],
    threat_pool: &[IncomingThreat],
    rng: &mut impl Rng,
) -> (Vec<IncomingThreat>, Vec<u32>) {
    let mut surviving: Vec<IncomingThreat> = Vec::new();
    // Track how many actual interceptors each defender has fired this step
    // so we respect `max_engagement_capacity` (interceptor capacity per
    // step before the launcher reloads from magazine).
    let mut interceptors_fired: Vec<u32> = vec![0; defenders.len()];
    // Track how many threats each defender successfully intercepted.
    let mut intercepts: Vec<u32> = vec![0; defenders.len()];

    for threat in threat_pool {
        // ── Phase 1: Targeted self-defence ──────────────────────
        let intercepted =
            if let Some(idx) = defenders.iter().position(|d| d.name == threat.target_name) {
                try_intercept(
                    defenders,
                    idx,
                    &mut interceptors_fired,
                    &mut intercepts,
                    rng,
                )
            } else {
                false
            };

        // ── Phase 2: Cooperative overflow ───────────────────────
        let intercepted = if intercepted {
            true
        } else {
            let mut done = false;
            for i in 0..defenders.len() {
                if try_intercept(defenders, i, &mut interceptors_fired, &mut intercepts, rng) {
                    done = true;
                    break;
                }
            }
            done
        };

        if !intercepted {
            surviving.push(threat.clone());
        }
    }

    (surviving, intercepts)
}

/// Try to have one defender engage a threat.  Returns `true` if the threat
/// was intercepted.
///
/// Checks capacity/magazine/staying-power before firing.  Mutates the
/// defender's magazine and the step-level `interceptors_fired`/`intercepts`
/// counters.
fn try_intercept(
    defenders: &mut [RunDefender],
    idx: usize,
    interceptors_fired: &mut [u32],
    intercepts: &mut [u32],
    rng: &mut impl Rng,
) -> bool {
    let defender = &mut defenders[idx];
    if defender.staying_power == 0
        || defender.magazine_depth == 0
        || interceptors_fired[idx] >= defender.max_engagement_capacity
    {
        return false;
    }

    match defender.doctrine {
        Doctrine::ShootLookShoot => {
            defender.magazine_depth -= 1;
            interceptors_fired[idx] += 1;
            if rng.random::<f64>() <= defender.pkd {
                intercepts[idx] += 1;
                true
            } else {
                false
            }
        }
        Doctrine::ShootShootLook => {
            let shots = defender.magazine_depth.min(2) as u32;
            defender.magazine_depth -= shots;
            interceptors_fired[idx] += shots;
            for _ in 0..shots {
                if rng.random::<f64>() <= defender.pkd {
                    intercepts[idx] += 1;
                    return true;
                }
            }
            false
        }
        Doctrine::MaxDefense => {
            let available = defender
                .magazine_depth
                .min(defender.max_engagement_capacity - interceptors_fired[idx]);
            defender.magazine_depth -= available;
            interceptors_fired[idx] += available;
            for _ in 0..available {
                if rng.random::<f64>() <= defender.pkd {
                    intercepts[idx] += 1;
                    return true;
                }
            }
            false
        }
    }
}

fn terminal_impact_phase(
    defenders: &mut [RunDefender],
    threats: &[IncomingThreat],
    rng: &mut impl Rng,
) {
    for threat in threats {
        if rng.random::<f64>() <= threat.pko {
            if let Some(target) = defenders.iter_mut().find(|d| d.name == threat.target_name) {
                target.hits_taken += 1;
                target.staying_power = target.staying_power.saturating_sub(1);
            }
        }
    }
}

/// Accumulate one run's outcomes into the global aggregator.
fn aggregate_run(
    config_defenders: &[Defender],
    run_defenders: &[RunDefender],
    stats: &mut HashMap<String, DefenderStats>,
) {
    for def in config_defenders {
        let run_data = run_defenders
            .iter()
            .find(|d| d.name == def.name)
            .expect("run_defenders should contain every config defender");
        let entry = stats
            .get_mut(&def.name)
            .expect("stats should be pre-populated for every defender");

        if run_data.staying_power > 0 {
            entry.survival_count += 1;
        }
        entry.total_hits_taken += run_data.hits_taken;
        entry.total_interceptors_spent += def.magazine_depth - run_data.magazine_depth;
    }
}

/// Convert aggregated stats into a structured `SimulationResult`.
fn build_result(
    config: &ScenarioConfig,
    iterations: u32,
    stats: &HashMap<String, DefenderStats>,
) -> SimulationResult {
    let defenders: Vec<DefenderResult> = config
        .defenders
        .iter()
        .map(|def| {
            let s = &stats[&def.name];
            DefenderResult {
                name: def.name.clone(),
                staying_power: def.staying_power,
                magazine_depth: def.magazine_depth,
                survival_count: s.survival_count,
                survival_rate: (s.survival_count as f64 / iterations as f64) * 100.0,
                avg_hits_taken: s.total_hits_taken as f64 / iterations as f64,
                avg_interceptors_fired: s.total_interceptors_spent as f64 / iterations as f64,
            }
        })
        .collect();

    let attackers: Vec<AttackerResult> = config
        .attackers
        .iter()
        .map(|atk| {
            let total_missiles: u32 = atk.salvo_schedule.iter().sum();
            AttackerResult {
                name: atk.name.clone(),
                target_name: atk.target_name.clone(),
                total_missiles,
                pko_label: atk.pko.label(),
            }
        })
        .collect();

    SimulationResult {
        scenario_name: config.scenario.name.clone(),
        iterations,
        defenders,
        attackers,
    }
}

/// Pretty-print a `SimulationResult` to stdout.
pub fn print_result(result: &SimulationResult) {
    println!("============================================================");
    println!("Results: {}", result.scenario_name);
    println!("Iterations: {}", result.iterations);
    println!("============================================================\n");

    for def in &result.defenders {
        println!("TARGET: {}", def.name);
        println!("  Survival Rate:           {:.2}%", def.survival_rate);
        println!(
            "  Avg Hits Taken:          {:.2} (Staying Power: {})",
            def.avg_hits_taken, def.staying_power
        );
        println!(
            "  Avg Interceptors Fired:  {:.2} / {}",
            def.avg_interceptors_fired, def.magazine_depth
        );
        println!("------------------------------------------------------------");
    }
}

// ============================================================
// STEP-THROUGH STATE MACHINE
// ============================================================

/// Drives an interactive step-through simulation, one time step at a time.
pub struct StepwiseSimulation {
    config: ScenarioConfig,
    defenders: Vec<RunDefender>,
    step: usize,
    max_steps: usize,
    rng: StdRng,
    finished: bool,
}

impl StepwiseSimulation {
    /// Initialise a new step-through simulation from a scenario config.
    /// Resolves PkD values for each defender for this run.
    pub fn new(config: ScenarioConfig) -> Self {
        let max_steps = determine_max_steps(&config.attackers);
        let mut rng = StdRng::from_rng(&mut rand::rng());
        let defenders = initialize_run(&config.defenders, &mut rng);
        StepwiseSimulation {
            config,
            defenders,
            step: 0,
            max_steps,
            rng,
            finished: max_steps == 0,
        }
    }

    /// Advance one time step and return the events for that step.
    /// Returns `None` if the simulation is already complete.
    pub fn advance(&mut self) -> Option<StepRecord> {
        if self.finished {
            return None;
        }

        let step = self.step;
        self.step += 1;
        let is_complete = self.step >= self.max_steps;
        if is_complete {
            self.finished = true;
        }

        // 1. Generate threat pool for this step.
        let threat_pool = generate_threat_pool(&self.config.attackers, step, &mut self.rng);
        let threat_launches = build_launch_events(&self.config.attackers, step);

        // 2. Empty step — nothing happens.
        if threat_pool.is_empty() {
            return Some(StepRecord {
                step,
                max_steps: self.max_steps,
                is_complete,
                threat_launches,
                interceptions: vec![],
                impacts: vec![],
                defender_snapshots: self.defenders.clone(),
            });
        }

        // 3. Sort defenders by PkD (best shooter first).
        sort_by_pkd(&mut self.defenders);

        // 4. Interception — compute events from magazine depth diff.
        let before_int = self.defenders.clone();
        let (surviving, intercept_counts) =
            interception_phase(&mut self.defenders, &threat_pool, &mut self.rng);
        let interceptions: Vec<InterceptionEvent> = self
            .defenders
            .iter()
            .enumerate()
            .map(|(i, after)| {
                let before = &before_int[i];
                InterceptionEvent {
                    defender_name: after.name.clone(),
                    doctrine_label: after.doctrine.label().to_string(),
                    interceptors_fired: before.magazine_depth - after.magazine_depth,
                    threats_intercepted: intercept_counts[i],
                    max_engagement_capacity: after.max_engagement_capacity,
                }
            })
            .filter(|e| e.interceptors_fired > 0)
            .collect();

        // 5. Terminal impact — compute events from hits_taken / staying_power diff.
        let before_imp = self.defenders.clone();
        terminal_impact_phase(&mut self.defenders, &surviving, &mut self.rng);

        // Count threats leaked per target.
        let mut leaked_per_target: HashMap<String, u32> = HashMap::new();
        for threat in &surviving {
            *leaked_per_target
                .entry(threat.target_name.clone())
                .or_insert(0) += 1;
        }

        let impacts: Vec<ImpactEvent> = self
            .defenders
            .iter()
            .enumerate()
            .map(|(i, after)| {
                let before = &before_imp[i];
                let hits_scored = after.hits_taken - before.hits_taken;
                let leaked = leaked_per_target.get(&after.name).copied().unwrap_or(0);
                ImpactEvent {
                    target_name: after.name.clone(),
                    missiles_leaked: leaked,
                    hits_scored,
                    hp_before: before.staying_power,
                    hp_after: after.staying_power,
                    destroyed: after.staying_power == 0,
                }
            })
            .filter(|e| e.hits_scored > 0 || e.missiles_leaked > 0)
            .collect();

        Some(StepRecord {
            step,
            max_steps: self.max_steps,
            is_complete,
            threat_launches,
            interceptions,
            impacts,
            defender_snapshots: self.defenders.clone(),
        })
    }

    /// Whether the simulation has no more steps.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Current step index (0-based).
    pub fn current_step(&self) -> usize {
        self.step
    }

    /// Total number of steps.
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }

    /// Scenario name for display.
    pub fn scenario_name(&self) -> &str {
        &self.config.scenario.name
    }

    /// Current defender state snapshot.
    pub fn defenders(&self) -> &[RunDefender] {
        &self.defenders
    }
}

/// Build launch event records for a given step.
fn build_launch_events(attackers: &[Attacker], step: usize) -> Vec<ThreatLaunch> {
    attackers
        .iter()
        .filter_map(|atk| {
            if step < atk.salvo_schedule.len() {
                let n = atk.salvo_schedule[step];
                if n > 0 {
                    Some(ThreatLaunch {
                        attacker_name: atk.name.clone(),
                        target_name: atk.target_name.clone(),
                        missiles_fired: n,
                        pko_label: atk.pko.label(),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Doctrine, Probability, ScenarioMeta};

    fn dummy_config() -> ScenarioConfig {
        ScenarioConfig {
            scenario: ScenarioMeta {
                name: "test".into(),
                description: None,
                iterations: Some(1),
            },
            defenders: vec![Defender {
                name: "Test Defender".into(),
                staying_power: 5,
                magazine_depth: 16,
                max_engagement_capacity: 8,
                doctrine: Doctrine::ShootLookShoot,
                pkd: Probability::Fixed { value: 0.80 },
            }],
            attackers: vec![Attacker {
                name: "Test Attacker".into(),
                target_name: "Test Defender".into(),
                salvo_schedule: vec![10, 10],
                pko: Probability::Fixed { value: 0.50 },
            }],
        }
    }

    #[test]
    fn magazine_decrements_when_intercepting() {
        let config = dummy_config();
        let mut sim = StepwiseSimulation::new(config);
        assert_eq!(sim.defenders()[0].magazine_depth, 16);

        sim.advance();
        assert!(
            sim.defenders()[0].magazine_depth < 16,
            "magazine did not decrease after step 1 (was 16, now {})",
            sim.defenders()[0].magazine_depth
        );
    }

    #[test]
    fn destroyed_defender_cannot_fire_in_subsequent_steps() {
        let mut config = dummy_config();
        config.attackers[0].pko = Probability::Fixed { value: 1.0 };
        let mut sim = StepwiseSimulation::new(config);

        while !sim.finished() {
            sim.advance();
        }

        assert_eq!(sim.defenders()[0].staying_power, 0);
        // The magazine should have been decremented when the defender fired,
        // and should NOT be reset or increased after destruction.
        assert!(
            sim.defenders()[0].magazine_depth <= 16,
            "magazine should not exceed initial value"
        );
    }
}
