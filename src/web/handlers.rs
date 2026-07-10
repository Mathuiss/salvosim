use rocket::form::{Form, FromForm};
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::{State, delete, get, post, routes, uri};
use rocket_dyn_templates::{Template, context};

use crate::config;
use crate::engine;
use crate::models::SimulationResult;

use super::AppState;
use super::scenario;

// ── Form types ───────────────────────────────────────────────────

#[derive(FromForm)]
pub struct ScenarioForm {
    pub filename: String,
    pub content: String,
}

fn strip_toml_ext(name: &str) -> String {
    name.strip_suffix(".toml").unwrap_or(name).to_string()
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
execution_mode = "monte_carlo"
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

// ── Run simulation ───────────────────────────────────────────────

#[post("/simulate/<name>")]
pub async fn run_simulation(name: &str, state: &State<AppState>) -> Result<Redirect, Status> {
    let dir = state.scenarios_dir.clone();
    let name_owned = name.to_string();

    // Run the simulation on a blocking thread (Monte Carlo is CPU-bound).
    let result: Option<SimulationResult> = rocket::tokio::task::spawn_blocking(move || {
        let path = dir.join(format!("{name_owned}.toml"));
        let config = config::load_scenario(&path.to_string_lossy()).ok()?;

        if config.scenario.execution_mode.to_lowercase() != "monte_carlo" {
            return None;
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
        run_simulation,
        view_results,
    ]
}
