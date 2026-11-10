//! Turning CS2's game state into a Discord activity.

use serde_json::{Map, Value, json};

const ASSETS: &str = "https://raw.githubusercontent.com/Tjabooo/cs2-presence/main/assets";
const MAP_IMAGES: &str = "https://raw.githubusercontent.com/ghostcap-gaming/cs2-map-images/main/cs2";

/// Modes without a team score.
const SCORELESS: [&str; 4] = ["deathmatch", "gungameprogressive", "training", "custom"];

/// Maps with a picture at MAP_IMAGES; any other map shows the CS2 icon.
const MAP_PICTURES: [&str; 32] = [
    "ar_baggage",
    "ar_pool_day",
    "ar_shoots",
    "cs_agency",
    "cs_alpine",
    "cs_italy",
    "cs_office",
    "de_ancient",
    "de_anubis",
    "de_basalt",
    "de_brewery",
    "de_cache",
    "de_dogtown",
    "de_dust2",
    "de_edin",
    "de_golden",
    "de_grail",
    "de_inferno",
    "de_jura",
    "de_mills",
    "de_mirage",
    "de_nuke",
    "de_overpass",
    "de_palacio",
    "de_poseidon",
    "de_rooftop",
    "de_sanctum",
    "de_stronghold",
    "de_thera",
    "de_train",
    "de_vertigo",
    "fy_pool_day",
];

const MAP_PREFIXES: [&str; 14] =
    ["de", "cs", "ar", "dz", "gd", "aim", "awp", "fy", "surf", "bhop", "kz", "ze", "zm", "workshop"];

fn mode_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "competitive" => "Competitive",
        "premier" => "Premier",
        "casual" => "Casual",
        "deathmatch" => "Deathmatch",
        "scrimcomp2v2" => "Wingman",
        "gungameprogressive" => "Arms Race",
        "gungametrbomb" => "Demolition",
        "training" => "Training",
        "survival" => "Danger Zone",
        "cooperative" => "Guardian",
        "coopmission" => "Co-op Strike",
        "skirmish" => "War Games",
        "custom" => "Custom",
        _ => return None,
    })
}

fn max_rounds(mode: &str) -> Option<i64> {
    match mode {
        "competitive" | "premier" => Some(24),
        "scrimcomp2v2" => Some(16),
        _ => None,
    }
}

fn team_name(team: &str) -> &'static str {
    if team == "CT" { "Counter-Terrorist" } else { "Terrorist" }
}

/// A number from the game state, whether sent as a number or a string.
fn int(value: &Value) -> i64 {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}

fn str_of(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

/// Python-style truthiness, as the game state uses it.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `"Title Case"` the way Python's `str.title` does it.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if after_letter {
            out.extend(c.to_lowercase());
        } else {
            out.extend(c.to_uppercase());
        }
        after_letter = c.is_alphabetic();
    }
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// `1234567` → `1,234,567`.
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// A readable map name: `de_dust2` → `Dust II`, `de_ancient_night` → `Ancient Night`.
pub fn map_title(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name).to_lowercase();
    match base.as_str() {
        "de_dust2" => return "Dust II".into(),
        "cs_italy" => return "Italy".into(),
        "cs_office" => return "Office".into(),
        "cs_agency" => return "Agency".into(),
        "ar_pool_day" => return "Pool Day".into(),
        "de_ancient_night" => return "Ancient Night".into(),
        _ => {}
    }
    let stripped = MAP_PREFIXES
        .iter()
        .find_map(|p| base.strip_prefix(p).and_then(|rest| rest.strip_prefix('_')))
        .unwrap_or(&base);
    let words: Vec<String> = stripped
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| if w.chars().all(|c| c.is_ascii_digit()) { w.to_string() } else { capitalize(w) })
        .collect();
    if words.is_empty() { name.to_string() } else { words.join(" ") }
}

/// Discord wants 2-128 characters per field.
pub fn text(value: &str) -> Option<String> {
    const LIMIT: usize = 128;
    let mut value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() > LIMIT {
        value = value.chars().take(LIMIT - 1).collect::<String>().trim_end().to_string() + "…";
    }
    (value.chars().count() >= 2).then_some(value)
}

/// Remembers what the game no longer reports: your own stats while you spectate after dying, and
/// when the current map started.
#[derive(Default)]
pub struct Tracker {
    map_key: Option<(Value, Value)>,
    map_started: Option<i64>,
    team: Option<String>,
    stats: Map<String, Value>,
}

impl Tracker {
    pub fn reset(&mut self) {
        *self = Tracker::default();
    }

    fn update(&mut self, data: &Value, now: i64) {
        let me = &data["provider"]["steamid"];
        let game_map = &data["map"];
        let key = truthy(game_map).then(|| (game_map["name"].clone(), game_map["mode"].clone()));
        if key != self.map_key {
            self.map_started = key.is_some().then_some(now);
            self.map_key = key;
            self.team = None;
            self.stats.clear();
        }
        let player = &data["player"];
        if truthy(me) && player["steamid"] == *me {
            if let Some(team) = player["team"].as_str().filter(|t| !t.is_empty()) {
                self.team = Some(team.to_string());
            }
            if let Some(stats) = player["match_stats"].as_object().filter(|s| !s.is_empty()) {
                self.stats = stats.clone();
            }
        }
    }
}

/// The activity for a game launched at `launched`, from the game state `data` (if CS2 sends it).
/// `now` is the current Unix time, used as the start of a new map.
pub fn build(launched: i64, data: Option<&Value>, tracker: &mut Tracker, now: i64) -> Value {
    let mut activity = json!({"type": 0, "timestamps": {"start": launched}});
    let Some(data) = data else {
        return activity;
    };
    tracker.update(data, now);
    let me = str_of(&data["provider"]["steamid"]);
    let player = &data["player"];
    let own = !me.is_empty() && player["steamid"] == me;
    let game_map = &data["map"];
    if !me.is_empty() {
        activity["buttons"] =
            json!([{"label": "Steam profile", "url": format!("https://steamcommunity.com/profiles/{me}")}]);
    }

    if !truthy(game_map) {
        activity["details"] = json!(match str_of(&player["activity"]) {
            "textinput" => "In the main menu (typing)",
            _ => "In the main menu",
        });
        if let Some(name) = player["name"].as_str().filter(|n| !n.is_empty()) {
            activity["state"] = json!(text(&format!("as {name}")));
        }
        return activity;
    }

    let mode_key = str_of(&game_map["mode"]);
    let mode = match mode_name(mode_key) {
        Some(mode) => mode.to_string(),
        None if mode_key.is_empty() => "Match".to_string(),
        None => title_case(&mode_key.replace('_', " ")),
    };
    let scoreless = SCORELESS.contains(&mode_key);
    let raw_map = str_of(&game_map["name"]);
    let title = map_title(raw_map);
    let phase = str_of(&game_map["phase"]);
    let rnd = &data["round"];
    let ct = int(&game_map["team_ct"]["score"]);
    let t = int(&game_map["team_t"]["score"]);
    let team = tracker.team.as_deref().filter(|t| *t == "CT" || *t == "T");
    let stats = &tracker.stats;
    if let Some(started) = tracker.map_started {
        activity["timestamps"] = json!({"start": started});
    }
    let ours = team.map(|team| if team == "CT" { (ct, t) } else { (t, ct) });

    // details: mode, map and score from your side.
    let score = match (scoreless, ours) {
        (true, _) => String::new(),
        (false, Some((us, them))) => format!(" · {us}–{them}"),
        (false, None) => format!(" · CT {ct}–{t} T"),
    };
    activity["details"] = json!(text(&format!("{mode} · {title}{score}")));

    // state: match phase and your scoreboard line.
    let stat = |key: &str| stats.get(key).map(int).unwrap_or(0);
    let (k, a, d, mvps, points) =
        (stat("kills"), stat("assists"), stat("deaths"), stat("mvps"), stat("score"));
    let line = if mode_key == "deathmatch" || mode_key == "gungameprogressive" {
        format!("{k} kills · {d} deaths · {points} pts")
    } else if !stats.is_empty() {
        let mvp = match mvps {
            0 => String::new(),
            1 => " · 1 MVP".to_string(),
            n => format!(" · {n} MVPs"),
        };
        format!("{k} / {a} / {d} K/A/D{mvp}")
    } else {
        String::new()
    };
    let rounds_done = int(&game_map["round"]);
    let round_now = rounds_done + i64::from(!(phase == "gameover" || rnd["phase"] == "over"));
    let max = max_rounds(mode_key);
    let prefix = match phase {
        "warmup" => "Warmup".to_string(),
        "intermission" => "Halftime".to_string(),
        "gameover" => match ours.filter(|_| !scoreless) {
            Some((us, them)) if us > them => "Won".to_string(),
            Some((us, them)) if us < them => "Lost".to_string(),
            Some(_) => "Draw".to_string(),
            None => "Match over".to_string(),
        },
        _ if scoreless => String::new(),
        _ => match max {
            Some(max) if round_now > max => format!("Overtime, round {round_now}"),
            Some(_) => String::new(),
            None => format!("Round {round_now}"),
        },
    };
    let state =
        [prefix.as_str(), line.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>();
    activity["state"] = json!(text(&state.join(" · ")).or_else(|| text(&mode)));

    // Round progress as the party size ("13 of 24") in modes with a fixed length.
    if let Some(max) = max
        && phase == "live"
        && (1..=max).contains(&round_now)
    {
        let me = if me.is_empty() { "local" } else { me };
        activity["party"] = json!({"id": format!("cs2-{me}-{raw_map}"), "size": [round_now, max]});
    }

    // Images: map picture (or the CS2 icon) and your team badge.
    let base = raw_map.rsplit('/').next().unwrap_or("").to_lowercase();
    let mut hover = vec![if !base.is_empty() && title.to_lowercase() != base {
        format!("{title} ({base})")
    } else {
        title.clone()
    }];
    if (phase == "live" || phase == "intermission") && !scoreless {
        hover.push(format!("Round {round_now}"));
    }
    match str_of(&rnd["bomb"]) {
        "planted" => hover.push("Bomb planted".into()),
        "defused" => hover.push("Bomb defused".into()),
        "exploded" => hover.push("Bomb exploded".into()),
        _ if rnd["phase"] == "freezetime" => hover.push("Freeze time".into()),
        _ => {}
    }
    let mut assets = Map::new();
    if MAP_PICTURES.contains(&base.as_str()) {
        assets.insert("large_image".into(), json!(format!("{MAP_IMAGES}/{base}.png")));
        assets.insert("large_text".into(), json!(text(&hover.join(" · "))));
        if let Some(team) = team {
            assets.insert("small_image".into(), json!(format!("{ASSETS}/{}.png?v=1", team.to_lowercase())));
            let state = &player["state"];
            let small = if own && int(&state["health"]) > 0 {
                let mut bits = vec![team_name(team).to_string(), format!("{} HP", int(&state["health"]))];
                let armor = int(&state["armor"]);
                if armor != 0 {
                    let helmet = if truthy(&state["helmet"]) { " + helmet" } else { "" };
                    bits.push(format!("{armor} armour{helmet}"));
                }
                if !state["money"].is_null() {
                    bits.push(format!("${}", thousands(int(&state["money"]))));
                }
                text(&bits.join(" · "))
            } else if phase == "live" && !own && !str_of(&player["name"]).is_empty() {
                text(&format!("{} · dead, watching {}", team_name(team), str_of(&player["name"])))
            } else {
                Some(team_name(team).to_string())
            };
            assets.insert("small_text".into(), json!(small));
        }
    }
    assets.retain(|_, v| !v.is_null());
    if !assets.is_empty() {
        activity["assets"] = Value::Object(assets);
    }
    activity
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "76561198000000001";

    fn state(map: Value, player: Value, round: Value) -> Value {
        json!({"provider": {"steamid": ME}, "map": map, "player": player, "round": round})
    }

    fn me(team: &str, health: i64, stats: Value) -> Value {
        json!({"steamid": ME, "name": "oliver", "team": team, "activity": "playing",
               "state": {"health": health, "armor": 100, "helmet": true, "money": 4250},
               "match_stats": stats})
    }

    fn comp(phase: &str, round: i64, ct: i64, t: i64) -> Value {
        json!({"mode": "competitive", "name": "de_dust2", "phase": phase, "round": round,
               "team_ct": {"score": ct}, "team_t": {"score": t}})
    }

    #[test]
    fn titles() {
        assert_eq!(map_title("de_dust2"), "Dust II");
        assert_eq!(map_title("workshop/123/de_cbble_classic"), "Cbble Classic");
        assert_eq!(map_title("de_train_2"), "Train 2");
        assert_eq!(map_title("cs_italy"), "Italy");
        assert_eq!(title_case("some mode9x"), "Some Mode9X");
        assert_eq!(thousands(16000), "16,000");
        assert_eq!(thousands(800), "800");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn text_limits() {
        assert_eq!(text("  a  b "), Some("a b".into()));
        assert_eq!(text("x"), None);
        let long = text(&"é".repeat(200)).unwrap();
        assert_eq!(long.chars().count(), 128);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn no_game_state() {
        let a = build(100, None, &mut Tracker::default(), 200);
        assert_eq!(a, json!({"type": 0, "timestamps": {"start": 100}}));
    }

    #[test]
    fn main_menu() {
        let data = json!({"provider": {"steamid": ME},
                          "player": {"steamid": ME, "name": "oliver", "activity": "menu"}});
        let a = build(100, Some(&data), &mut Tracker::default(), 200);
        assert_eq!(a["details"], "In the main menu");
        assert_eq!(a["state"], "as oliver");
        assert_eq!(a["timestamps"]["start"], 100);
        assert_eq!(a["buttons"][0]["url"], format!("https://steamcommunity.com/profiles/{ME}"));
    }

    #[test]
    fn competitive_round() {
        let stats = json!({"kills": 7, "assists": 2, "deaths": 3, "mvps": 2, "score": 20});
        let data =
            state(comp("live", 4, 2, 3), me("T", 87, stats), json!({"phase": "live", "bomb": "planted"}));
        let mut tracker = Tracker::default();
        let a = build(100, Some(&data), &mut tracker, 200);
        assert_eq!(a["details"], "Competitive · Dust II · 3–2");
        assert_eq!(a["state"], "7 / 2 / 3 K/A/D · 2 MVPs");
        assert_eq!(a["party"], json!({"id": format!("cs2-{ME}-de_dust2"), "size": [5, 24]}));
        assert_eq!(a["timestamps"]["start"], 200);
        assert_eq!(a["assets"]["large_image"], format!("{MAP_IMAGES}/de_dust2.png"));
        assert_eq!(a["assets"]["large_text"], "Dust II (de_dust2) · Round 5 · Bomb planted");
        assert_eq!(a["assets"]["small_image"], format!("{ASSETS}/t.png?v=1"));
        assert_eq!(a["assets"]["small_text"], "Terrorist · 87 HP · 100 armour + helmet · $4,250");
        // The map start stays put on later updates.
        assert_eq!(build(100, Some(&data), &mut tracker, 999)["timestamps"]["start"], 200);
    }

    #[test]
    fn spectating_keeps_own_stats() {
        let mut tracker = Tracker::default();
        let stats = json!({"kills": 1, "assists": 0, "deaths": 0, "mvps": 1, "score": 2});
        build(0, Some(&state(comp("live", 0, 0, 0), me("CT", 100, stats), json!({}))), &mut tracker, 0);
        let other = json!({"steamid": "2", "name": "friend", "team": "CT", "state": {"health": 50},
                           "match_stats": {"kills": 9}});
        let a = build(
            0,
            Some(&state(comp("live", 0, 0, 0), other, json!({"phase": "freezetime"}))),
            &mut tracker,
            0,
        );
        assert_eq!(a["state"], "1 / 0 / 0 K/A/D · 1 MVP");
        assert_eq!(a["assets"]["small_text"], "Counter-Terrorist · dead, watching friend");
        assert_eq!(a["assets"]["large_text"], "Dust II (de_dust2) · Round 1 · Freeze time");
    }

    #[test]
    fn phases() {
        let mut tracker = Tracker::default();
        let p = me("CT", 0, json!({"kills": 0}));
        let over = state(comp("gameover", 23, 13, 10), p.clone(), json!({"phase": "over"}));
        let a = build(0, Some(&over), &mut tracker, 0);
        assert_eq!(a["state"], "Won · 0 / 0 / 0 K/A/D");
        assert!(a.get("party").is_none());
        assert_eq!(a["assets"]["small_text"], "Counter-Terrorist");
        let ot = state(comp("live", 24, 12, 12), p.clone(), json!({}));
        assert!(
            build(0, Some(&ot), &mut tracker, 0)["state"].as_str().unwrap().starts_with("Overtime, round 25")
        );
        let warm = state(comp("warmup", 0, 0, 0), p, json!({}));
        assert!(build(0, Some(&warm), &mut tracker, 0)["state"].as_str().unwrap().starts_with("Warmup"));
    }

    #[test]
    fn deathmatch_and_unknown_maps() {
        let map = json!({"mode": "deathmatch", "name": "de_mirage", "phase": "live", "round": 0});
        let p = me("T", 100, json!({"kills": 12, "deaths": 4, "score": 30}));
        let a = build(0, Some(&state(map, p.clone(), json!({}))), &mut Tracker::default(), 0);
        assert_eq!(a["details"], "Deathmatch · Mirage");
        assert_eq!(a["state"], "12 kills · 4 deaths · 30 pts");
        let map = json!({"mode": "new_mode", "name": "workshop/1/aim_map", "phase": "live", "round": 2});
        let a = build(0, Some(&state(map, json!({}), json!({}))), &mut Tracker::default(), 0);
        assert_eq!(a["details"], "New Mode · Map · CT 0–0 T");
        assert_eq!(a["state"], "Round 3");
        assert!(a.get("assets").is_none());
    }
}
