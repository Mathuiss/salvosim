# Salvosim — Universal Salvo Simulator

Simulate missile-defence engagements using [Hughe's salvo model](https://en.wikipedia.org/wiki/Salvo_combat_model).
Salvosim models a battle in discrete time-steps: each step, attackers launch a salvo
of missiles toward defender platforms; defenders detect, track, and fire
interceptors; surviving missiles arrive on target and roll for damage.
Results are aggregated over thousands of Monte Carlo iterations to give
statistically stable predictions.

## When and why to use Salvosim

Salvosim is useful whenever you need to reason about a missile-on-missile
engagement where the outcome depends on probabilistic events:

- **Force structure analysis.**  How many interceptors does a ship need to
  survive a saturation strike?  What happens if magazine depth is cut in
  half?  Salvosim gives you survival-rate curves, not single-point answers.

- **Doctrine comparison.**  Does *Shoot-Look-Shoot* (assess each intercept
  before committing the next interceptor) beat *Max Defense* (fire all
  capacity at once) for a given threat profile?  Run both and compare
  survival rates.

- **Red-force / blue-force wargaming.**  Model an IADS (Integrated Air
  Defence System) with layered SAM batteries, decoys, SEAD, and precision
  munitions.  Vary the attacker's salvo schedule and PKO to find the
  cheapest salvo that achieves a kill.

- **Teaching and demonstration.**  Step through an engagement turn-by-turn
  in the interactive web UI to show exactly how interceptor allocation,
  PKO rolls, and terminal impact cascade.

Salvosim is **not** a physics-level flight simulator.  It operates at the
salvo-engagement level: missiles are abstract quantities, PKO/PkD are
single-roll probabilities, and geometry is implicit in the probability
distributions.  This makes it fast (10 000 iterations in under a second)
and composable, but unsuitable for questions about radar horizons, fly-out
times, or chaff / flare / kinematic defeat.

## Concepts and terminology

### Hughe's salvo model

The simulation is built on a discrete-time, stochastic model where one
time-step corresponds to one salvo exchange:

```
Step t:  Attackers launch → Defenders intercept → Terminal impact → Step t+1
```

Each step is evaluated independently within one Monte Carlo iteration.
All random draws (PkD, PKO) are seeded per-iteration for reproducibility.

### Defenders

A defender is a platform (ship, SAM battery, bunker) with four attributes:

| Field                     | Meaning                                                            |
| ------------------------- | ------------------------------------------------------------------ |
| `staying_power`           | Hit points — how many leakers it can absorb before it is destroyed |
| `magazine_depth`          | Total interceptor missiles available for the entire engagement     |
| `max_engagement_capacity` | Maximum interceptors that can be fired per step                    |
| `pkd`                     | Probability of Kill **per interceptor** against an incoming threat |

A defender whose `staying_power` reaches zero is destroyed and takes no
further part in the engagement.

### Attackers

An attacker is a missile type or launch platform with:

| Field            | Meaning                                                                                                  |
| ---------------- | -------------------------------------------------------------------------------------------------------- |
| `target_name`    | Which defender this attacker fires at (must match a defender name)                                       |
| `salvo_schedule` | Number of missiles fired per step, e.g. `[8, 4, 0, 0]` means 8 in step 0, 4 in step 1, none in steps 2–3 |
| `pko`            | Probability of Kill **per missile** that survives interception                                           |

The salvo schedule determines how many time-steps the simulation runs (the
length of the longest schedule across all attackers).

### Probability distributions

PkD and PKO can be specified in three ways:

| TOML form                                                       | Description                                  |
| --------------------------------------------------------------- | -------------------------------------------- |
| `{ type = "fixed", value = 0.80 }`                              | A single constant value used in every draw   |
| `{ type = "uniform_range", min = 0.60, max = 0.90 }`            | Uniform random between min and max each roll |
| `{ type = "normal_distribution", mean = 0.75, std_dev = 0.05 }` | Normal (Gaussian) draw clamped to [0, 1]     |

### Doctrines

| Doctrine             | Behaviour                                                                                                                                                                                                                                                                                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `"shoot-look-shoot"` | The defender fires **one** interceptor at a threat, checks PkD, and only fires a second if the first missed.  This conserves magazine depth but may leave threats unengaged when capacity is exhausted.                                                                                                                                                         |
| `"shoot-shoot-look"` | The defender salvo-launches **two** interceptors simultaneously per threat.  Both are committed before the outcome of the first is known (simulates a simultaneous salvo engagement).  If any interceptor hits, the threat is intercepted.  Gives a higher per-threat kill probability than Shoot-Look-Shoot at the cost of more magazine depth per engagement. |
| `"max_defense"`      | The defender fires **all remaining engagement capacity** at the same threat simultaneously.  If any interceptor hits, the threat is intercepted.  This maximises the chance of a kill per threat but burns magazine depth quickly.                                                                                                                              |

### Simulation loop (per iteration)

1. **Initialise** — copy each defender into a `RunDefender` with a fresh PkD
   draw from its probability distribution.  Reset magazine, staying power,
   and threat pool.

2. **For each time-step** (0 .. max steps):
   a. **Generate threats** — each attacker with a non-zero salvo in this
      step contributes that many incoming threats, each targeted at its
      configured defender.
   b. **Sort defenders** by PkD descending (most effective shooters engage
      first).
   c. **Interception phase** — for each defender in sorted order:
      - Group all threats targeting this defender.
      - Apply the defender's doctrine:
        *Shoot-Look-Shoot*: for each threat, fire one interceptor, roll
        PkD.  If hit, the threat is intercepted and the next threat is
        considered.  If miss and capacity remains, fire another.
        *Shoot-Shoot-Look*: salvo-launch up to two interceptors simultaneously
        per threat.  Both are committed before the outcome is known; if any
        interceptor hits, the threat is intercepted.
        *Max Defense*: fire all remaining engagement capacity at the first
        threat.  If any roll hits, the threat is intercepted.
      - Magazine is decremented by the number of interceptors actually
        fired.
   d. **Terminal impact** — each surviving threat rolls PKO against its
      target defender.  A successful roll reduces the defender's
      `staying_power` by 1.

3. **Record** — did each defender survive this iteration?  How many hits
   did it take?  How many interceptors did it fire?

4. **Aggregate** across all iterations to produce survival rates, average
   hits taken, and average interceptors fired.

## Installation

### Prerequisites

- **Rust toolchain** (edition 2024, minimum Rust 1.85+).
  Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

### Build from source

```bash
git clone <repo-url> salvosim
cd salvosim
cargo build --release
```

The binary is placed at `target/release/salvosim`.

### Run tests

```bash
cargo test
```

## Usage

### Command-line interface

```
salvosim [OPTIONS] [FILE]
salvosim serve [ADDR]
```

**Run a Monte Carlo simulation:**
```bash
salvosim scenarios/scenario-mc.toml
```
This runs the scenario file, prints a compact summary to stdout, and exits.

**Launch the web interface:**
```bash
salvosim serve
```
Opens the web dashboard at `http://localhost:8080`.  Accept an optional
custom address:
```bash
salvosim serve 127.0.0.1:9090
```

**Step-through (interactive) mode:**
```bash
salvosim scenarios/scenario-st.toml
```
*Not yet implemented on the CLI — use the web interface's Walk button instead.*

### Web interface

The web server provides a full GUI for creating, editing, running, and
analysing scenarios.

| Route                       | Purpose                                                             |
| --------------------------- | ------------------------------------------------------------------- |
| `/`                         | Dashboard — list all scenarios with Run, Walk, Edit, Delete buttons |
| `/scenarios/new`            | Create a new scenario using the structured form                     |
| `/scenarios/<name>/edit`    | Edit an existing scenario                                           |
| `/simulate/<name>`          | Run Monte Carlo (10 000 iteration default) and redirect to results  |
| `/results/<name>`           | View results — survival rates, hits taken, interceptor expenditure  |
| `/interactive/<name>/start` | Begin an interactive step-through session                           |
| `/session/<id>/advance`     | Advance one time-step and return updated state + graph data         |
| `/session/<id>`             | View the current interactive session                                |

#### Structured editor

The edit form provides a card-based layout:

- **Scenario metadata** — name, description, iteration count.
- **Defenders** — each card has name, staying power, magazine, engagement
  capacity, doctrine selector, and PkD probability (Fixed / Uniform Range /
  Normal Distribution with conditional fields that toggle based on the
  selected type).
- **Attackers** — each card has name, target (dropdown populated from
  current defender names with free-text fallback), salvo schedule
  (comma-separated), and PKO probability.
- **Add / Remove** — buttons to add or delete defender and attacker cards.
  Removing a defender also removes its name from the attacker target
  dropdown automatically.

On save, the form serialises its state to JSON, the server validates and
converts it to TOML, and writes it to `scenarios/<slugified-name>.toml`.

#### Results view

After a Monte Carlo run, the results page shows a table of defenders with:

- Survival count and rate (out of total iterations)
- Average hits taken per iteration
- Average interceptors fired per iteration
- Magazine depth and staying power

An attacker summary table shows total missiles committed and PKO profile.

## Scenario file format

Scenario files are TOML.  A complete example:

```toml
[scenario]
name = "Operation Distant Thunder - Multi-Axis Strike"
description = "10,000 iteration Monte Carlo testing a coordinated multi-domain strike."
iterations = 10000

[[defenders]]
name = "Aegis Cruiser (Primary)"
staying_power = 4
magazine_depth = 48
max_engagement_capacity = 12
doctrine = "shoot-look-shoot"
pkd = { type = "uniform_range", min = 0.65, max = 0.85 }

[[defenders]]
name = "Escort Destroyer"
staying_power = 3
magazine_depth = 32
max_engagement_capacity = 8
doctrine = "shoot-look-shoot"
pkd = { type = "uniform_range", min = 0.60, max = 0.80 }

[[attackers]]
name = "Submarine Strike (Cruise Missiles)"
target_name = "Aegis Cruiser (Primary)"
salvo_schedule = [24, 0]
pko = { type = "normal_distribution", mean = 0.80, std_dev = 0.05 }

[[attackers]]
name = "Air Wing Strike (Anti-Ship Missiles)"
target_name = "Escort Destroyer"
salvo_schedule = [0, 16]
pko = { type = "normal_distribution", mean = 0.75, std_dev = 0.08 }
```

### `[scenario]`

| Key           | Type    | Required | Description                                             |
| ------------- | ------- | -------- | ------------------------------------------------------- |
| `name`        | string  | yes      | Display name shown in the UI                            |
| `description` | string  | no       | Longer description                                      |
| `iterations`  | integer | no       | Monte Carlo iterations (defaults to 1000 in the web UI) |

### `[[defenders]]`

| Key                       | Type    | Required | Description                                                    |
| ------------------------- | ------- | -------- | -------------------------------------------------------------- |
| `name`                    | string  | yes      | Unique identifier; other fields reference this name            |
| `staying_power`           | integer | yes      | Hit points before the defender is destroyed (≥ 1)              |
| `magazine_depth`          | integer | yes      | Total interceptor inventory for the entire engagement          |
| `max_engagement_capacity` | integer | yes      | Maximum interceptors that can be fired per time-step           |
| `doctrine`                | string  | yes      | `"shoot-look-shoot"`, `"shoot-shoot-look"`, or `"max_defense"` |
| `pkd`                     | table   | yes      | PkD probability distribution (see below)                       |

### `[[attackers]]`

| Key              | Type              | Required | Description                                      |
| ---------------- | ----------------- | -------- | ------------------------------------------------ |
| `name`           | string            | yes      | Display name                                     |
| `target_name`    | string            | yes      | Must match a defender's `name` exactly           |
| `salvo_schedule` | array of integers | yes      | Missile count per time-step, e.g. `[8, 4, 0, 0]` |
| `pko`            | table             | yes      | PKO probability distribution (see below)         |

### Probability table

| Variant                                                         | Fields                                                          |
| --------------------------------------------------------------- | --------------------------------------------------------------- |
| `{ type = "fixed", value = 0.80 }`                              | `value` — constant (0.0 – 1.0)                                  |
| `{ type = "uniform_range", min = 0.60, max = 0.90 }`            | `min`, `max` — range bounds (0.0 – 1.0)                         |
| `{ type = "normal_distribution", mean = 0.75, std_dev = 0.05 }` | `mean`, `std_dev` — normal parameters; result clamped to [0, 1] |

All probability values are in **decimal** (0.80 = 80 %).

## Architecture overview

```
┌─────────────┐      ┌─────────────┐      ┌──────────────┐
│  scenario   │────▶│  engine     │────▶│  results     │
│  .toml      │      │  (Monte     │      │  (stdout /   │
│             │      │   Carlo)    │      │   web page)  │
└─────────────┘      └─────────────┘      └──────────────┘
       │                    ▲
       ▼                    │
┌─────────────┐      ┌──────────────┐
│  web server │────▶│  stepwise    │
│  (Rocket)   │      │  simulation  │
│             │      │  (interactiv)│
└─────────────┘      └──────────────┘
```

The codebase is organised into four modules plus a web sub-module:

| Module            | Responsibility                                             |
| ----------------- | ---------------------------------------------------------- |
| `config.rs`       | Load and validate TOML scenario files                      |
| `models.rs`       | All data types: config, runtime state, aggregation, output |
| `engine.rs`       | Monte Carlo engine, stepwise simulation, doctrine dispatch |
| `error.rs`        | Error types (`SalvoSimError`, `ConfigLoadError`)           |
| `web/mod.rs`      | Rocket configuration, `AppState`, session management       |
| `web/handlers.rs` | Route handlers (REST + HTMX)                               |
| `web/scenario.rs` | Scenario file listing, reading, saving, deletion           |
| `main.rs`         | CLI entry point (subcommands `simulate` / `serve`)         |

## Example scenarios

The `scenarios/` directory ships with several pre-built examples:

| File                 | Description                                                                                   |
| -------------------- | --------------------------------------------------------------------------------------------- |
| `scenario-mc.toml`   | Carrier strike group vs. submarine and air-wing attack (10 000 iterations)                    |
| `scenario-st.toml`   | Lightweight two-defender setup for step-through demonstration                                 |
| `scenario-iads.toml` | Four-wave IADS penetration with decoys, SEAD, cruise missiles, and precision bunker munitions |

These are automatically copied into `scenarios/` when the web server starts
for the first time.

## Advanced tips

- **Designing attacker waves.**  The `salvo_schedule` lets you model
  time-phased attacks.  Wave 1: decoys to waste defender magazines.  Wave
  2: cruise missiles to degrade long-range SAMs.  Wave 3: penetrating
  bombers against the weakened inner layer.  Each wave targets a different
  defender, and later waves arrive after earlier waves have depleted
  magazines.

- **Modelling decoys.**  Set PKO to 0.0 or very low.  The decoy still
  consumes defender interceptor capacity and magazine depth, just like a
  real threat.  This is the defining tactic of IADS saturation.

- **Comparing doctrines.**  Create two copies of the same scenario with
  different defender doctrines.  Run both and compare survival rates.
  *Shoot-Look-Shoot* tends to conserve ammunition but lets more threats
  through under high saturation; *Shoot-Shoot-Look* trades magazine depth
  for higher per-threat kill probability; *Max Defense* stops more threats
  per step but runs out of ammunition fastest.

- **Finding breakpoints.**  Start with conservative defender values and
  reduce magazine depth or engagement capacity until survival rate drops
  below 50 %.  That crossing point is the minimum required capability.

## License

See `LICENSE`.

## Building and contributing

```bash
cargo build --release    # production binary
cargo test               # run unit and integration tests
cargo fmt --all          # format code
cargo clippy             # lint
```

The engine is deliberately kept free of external dependencies on anything
but `rand` and `serde`.  The web server is optional (Rocket, Tera) and is
linked only when `salvosim serve` is the entry point.
