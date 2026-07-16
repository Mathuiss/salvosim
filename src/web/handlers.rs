use rocket::form::{Form, FromForm};
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::{State, delete, get, post, routes, uri};
use rocket_dyn_templates::{Template, context};

use crate::config;
use crate::engine;
use crate::engine::StepwiseSimulation;
use crate::models::SimulationResult;

use super::scenario;
use super::{AppState, InteractiveSession, generate_session_id};

// ── Form types ───────────────────────────────────────────────────

#[derive(FromForm)]
pub struct ScenarioForm {
    pub filename: String,
    pub content: String,
}

fn strip_toml_ext(name: &str) -> String {
    name.strip_suffix(".toml").unwrap_or(name).to_string()
}

#[derive(FromForm)]
pub struct SimulateForm {
    pub iterations: u32,
}

// ── Dashboard ────────────────────────────────────────────────────

#[get("/")]
pub fn index(state: &State<AppState>) -> Template {
    let scenarios = scenario::list_scenarios(&state.scenarios_dir);
    Template::render("index", context! { scenarios: scenarios })
}

// ── Scenario listing ─────────────────────────────────────────────

#[get("/scenarios")]
pub fn list_scenarios(state: &State<AppState>) -> Template {
    let scenarios = scenario::list_scenarios(&state.scenarios_dir);
    Template::render("scenarios", context! { scenarios: scenarios })
}

// ── Scenario creation form ───────────────────────────────────────

#[get("/scenarios/new")]
pub fn new_scenario_form() -> Template {
    let template = r#"[scenario]
name = "My Scenario"
description = ""
iterations = 1000

[[defenders]]
name = "Defender"
staying_power = 4
magazine_depth = 48
max_engagement_capacity = 12
doctrine = "shoot-look-shoot"
pkd = { type = "fixed", value = 0.70 }

[[attackers]]
name = "Attacker"
target_name = "Defender"
missile_inventory = 24
salvo_schedule = [24, 0]
pko = { type = "fixed", value = 0.80 }
"#;
    Template::render(
        "scenario_form",
        context! {
            is_new: true,
            name: "",
            content: template,
        },
    )
}

// ── Scenario edit form ───────────────────────────────────────────

#[get("/scenarios/<name>/edit")]
pub fn edit_scenario_form(name: &str, state: &State<AppState>) -> Result<Template, Status> {
    let content =
        scenario::read_scenario_raw(&state.scenarios_dir, name).map_err(|_| Status::NotFound)?;
    Ok(Template::render(
        "scenario_form",
        context! {
            is_new: false,
            name,
            content,
        },
    ))
}

// ── Save new scenario ────────────────────────────────────────────

#[post("/scenarios", data = "<form>")]
pub fn save_scenario(
    form: Form<ScenarioForm>,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    let name = strip_toml_ext(&form.filename);
    scenario::save_scenario(&state.scenarios_dir, &name, &form.content)
        .map_err(|_| Status::BadRequest)?;
    Ok(Redirect::to(uri!("/scenarios")))
}

// ── Update existing scenario ─────────────────────────────────────

#[post("/scenarios/<name>", data = "<form>")]
pub fn update_scenario(
    name: &str,
    form: Form<ScenarioForm>,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    // The `<name>` in the path is the canonical name; use filename from form for rename.
    let new_name = strip_toml_ext(&form.filename);

    // If the target name changed, delete the old file first.
    if new_name != name {
        scenario::delete_scenario(&state.scenarios_dir, name).ok();
    }

    scenario::save_scenario(&state.scenarios_dir, &new_name, &form.content)
        .map_err(|_| Status::BadRequest)?;
    Ok(Redirect::to(uri!("/scenarios")))
}

// ── Delete scenario ──────────────────────────────────────────────

#[delete("/scenarios/<name>")]
pub fn delete_scenario(name: &str, state: &State<AppState>) -> Result<Redirect, Status> {
    scenario::delete_scenario(&state.scenarios_dir, name).map_err(|_| Status::NotFound)?;
    Ok(Redirect::to(uri!("/")))
}

// ── Run configuration page ───────────────────────────────────────

#[get("/run/<name>")]
pub async fn run_page(name: &str, state: &State<AppState>) -> Result<Template, Status> {
    let content =
        scenario::read_scenario_raw(&state.scenarios_dir, name).map_err(|_| Status::NotFound)?;

    let value: toml::Value = toml::from_str(&content).map_err(|_| Status::NotFound)?;
    let default_iterations: u32 = value
        .get("scenario")
        .and_then(|s| s.get("iterations"))
        .and_then(|i| i.as_integer())
        .map(|i| i as u32)
        .unwrap_or(1000);

    Ok(Template::render(
        "run",
        context! {
            name,
            default_iterations,
        },
    ))
}

// ── Run simulation (accepts optional iterations override) ────────

#[post("/simulate/<name>", data = "<form>")]
pub async fn run_simulation(
    name: &str,
    form: Option<Form<SimulateForm>>,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    let dir = state.scenarios_dir.clone();
    let name_owned = name.to_string();
    let iterations_override = form.as_ref().map(|f| f.iterations);

    // Run the simulation on a blocking thread (Monte Carlo is CPU-bound).
    let result: Option<SimulationResult> = rocket::tokio::task::spawn_blocking(move || {
        let path = dir.join(format!("{name_owned}.toml"));
        let mut config = config::load_scenario(&path.to_string_lossy()).ok()?;
        if let Some(iters) = iterations_override {
            config.scenario.iterations = Some(iters);
        }
        Some(engine::run_monte_carlo(&config))
    })
    .await
    .map_err(|_| Status::InternalServerError)?;

    let result = result.ok_or(Status::BadRequest)?;

    // Store for the results page.
    state
        .results
        .write()
        .unwrap()
        .insert(name.to_string(), result);
    Ok(Redirect::to(format!("/results/{name}")))
}

// ── View results ─────────────────────────────────────────────────

#[get("/results/<name>")]
pub fn view_results(name: &str, state: &State<AppState>) -> Result<Template, Status> {
    let results = state.results.read().unwrap();
    let result = results.get(name).ok_or(Status::NotFound)?;
    Ok(Template::render("results", context! { result }))
}

// ── Interactive step-through ─────────────────────────────────────

#[post("/interactive/<name>/start")]
pub async fn interactive_start(name: &str, state: &State<AppState>) -> Result<Redirect, Status> {
    let dir = state.scenarios_dir.clone();
    let name_owned = name.to_string();

    // Load scenario config from disk.
    let config = rocket::tokio::task::spawn_blocking(move || {
        let path = dir.join(format!("{name_owned}.toml"));
        config::load_scenario(&path.to_string_lossy()).ok()
    })
    .await
    .map_err(|_| Status::InternalServerError)?
    .ok_or(Status::NotFound)?;

    let session_id = generate_session_id();
    let sim = StepwiseSimulation::new(config);
    let display_name = sim.scenario_name().to_string();
    let file_name = name.to_string();

    let initial_snapshot = sim.defenders().to_vec();

    state.sessions.write().unwrap().insert(
        session_id.clone(),
        InteractiveSession {
            sim,
            history: Vec::new(),
            initial_snapshot,
            scenario_file: file_name,
        },
    );
    state
        .session_scenario
        .write()
        .unwrap()
        .insert(session_id.clone(), display_name);

    Ok(Redirect::to(format!("/interactive/{session_id}")))
}

#[get("/interactive/<session_id>")]
pub fn interactive_view(session_id: &str, state: &State<AppState>) -> Result<Template, Status> {
    let sessions = state.sessions.read().unwrap();
    // We hold the read lock across the render; the template is Sync-bound.
    let sess = sessions.get(session_id).ok_or(Status::NotFound)?;
    let scenario_name = state
        .session_scenario
        .read()
        .unwrap()
        .get(session_id)
        .cloned()
        .unwrap_or_default();

    let record = sess.history.last();
    let initial = &sess.initial_snapshot;

    // ── Pre-compute chart data from history ──────────────────────────
    // HP over time: one array per defender starting with initial HP.
    let mut hp_names: Vec<&str> = Vec::new();
    let mut hp_series: Vec<Vec<u32>> = Vec::new();
    for d in initial {
        hp_names.push(&d.name);
        hp_series.push(vec![d.staying_power]);
    }
    for rec in &sess.history {
        for (i, name) in hp_names.iter().enumerate() {
            let hp = rec
                .defender_snapshots
                .iter()
                .find(|s| s.name == *name)
                .map(|s| s.staying_power)
                .unwrap_or(0);
            hp_series[i].push(hp);
        }
    }

    // Missile flow per step: incoming vs intercepted vs survived.
    let mut mf_step: Vec<usize> = Vec::new();
    let mut mf_incoming: Vec<u32> = Vec::new();
    let mut mf_intercepted: Vec<u32> = Vec::new();
    let mut mf_survived: Vec<u32> = Vec::new();
    let mut int_step: Vec<usize> = Vec::new();
    let mut int_fired: Vec<u32> = Vec::new();
    for rec in &sess.history {
        let incoming: u32 = rec.threat_launches.iter().map(|t| t.missiles_fired).sum();
        let survived: u32 = rec.impacts.iter().map(|i| i.missiles_leaked).sum::<u32>();
        let intercepted = incoming.saturating_sub(survived);
        mf_step.push(rec.step + 1);
        mf_incoming.push(incoming);
        mf_intercepted.push(intercepted);
        mf_survived.push(survived);

        let fired: u32 = rec.interceptions.iter().map(|i| i.interceptors_fired).sum();
        int_step.push(rec.step + 1);
        int_fired.push(fired);
    }

    Ok(Template::render(
        "interactive",
        context! {
            session_id,
            scenario_name: scenario_name,
            step: sess.sim.current_step(),
            max_steps: sess.sim.max_steps(),
            is_finished: sess.sim.finished(),
            defenders: sess.sim.defenders(),
            record,
            history: &sess.history,
            // chart series
            hp_names,
            hp_series,
            mf_step,
            mf_incoming,
            mf_intercepted,
            mf_survived,
            int_step,
            int_fired,
        },
    ))
}

#[post("/interactive/<session_id>/advance")]
pub async fn interactive_advance(
    session_id: &str,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    // We need to extract the sim, advance it, and put it back.
    // To avoid holding a write lock across await, take it out, advance, reinsert.
    let mut sessions = state.sessions.write().unwrap();
    let mut sess = sessions.remove(session_id).ok_or(Status::NotFound)?;

    // Advance is synchronous and fast — no spawn_blocking needed.
    if let Some(record) = sess.sim.advance() {
        sess.history.push(record);
    }

    sessions.insert(session_id.to_string(), sess);
    drop(sessions);

    Ok(Redirect::to(format!("/interactive/{session_id}")))
}

#[post("/interactive/<session_id>/reset")]
pub async fn interactive_reset(
    session_id: &str,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    // Read the stored scenario file name from the existing session.
    let scenario_file = {
        let sessions = state.sessions.read().unwrap();
        let sess = sessions.get(session_id).ok_or(Status::NotFound)?;
        sess.scenario_file.clone()
    };

    let dir = state.scenarios_dir.clone();
    let sf = scenario_file.clone();

    let config = rocket::tokio::task::spawn_blocking(move || {
        let path = dir.join(format!("{sf}.toml"));
        config::load_scenario(&path.to_string_lossy()).ok()
    })
    .await
    .map_err(|_| Status::InternalServerError)?
    .ok_or(Status::NotFound)?;

    let mut sessions = state.sessions.write().unwrap();
    let sim = StepwiseSimulation::new(config);
    let initial_snapshot = sim.defenders().to_vec();
    sessions.insert(
        session_id.to_string(),
        InteractiveSession {
            sim,
            history: Vec::new(),
            initial_snapshot,
            scenario_file,
        },
    );

    Ok(Redirect::to(format!("/interactive/{session_id}")))
}

#[post("/interactive/<session_id>/finish")]
pub async fn interactive_finish(
    session_id: &str,
    state: &State<AppState>,
) -> Result<Redirect, Status> {
    let mut sessions = state.sessions.write().unwrap();
    let mut sess = sessions.remove(session_id).ok_or(Status::NotFound)?;

    // Advance all remaining steps.
    while let Some(record) = sess.sim.advance() {
        sess.history.push(record);
    }

    sessions.insert(session_id.to_string(), sess);
    drop(sessions);

    Ok(Redirect::to(format!("/interactive/{session_id}")))
}

// ── Route list ───────────────────────────────────────────────────

pub fn routes() -> Vec<rocket::Route> {
    routes![
        index,
        list_scenarios,
        new_scenario_form,
        edit_scenario_form,
        save_scenario,
        update_scenario,
        delete_scenario,
        run_page,
        run_simulation,
        view_results,
        interactive_start,
        interactive_view,
        interactive_advance,
        interactive_reset,
        interactive_finish,
    ]
}
