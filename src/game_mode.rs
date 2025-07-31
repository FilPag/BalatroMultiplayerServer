use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    #[serde(rename = "gamemode_mp_attrition")]
    Attrition,
    #[serde(rename = "gamemode_mp_showdown")]
    Showdown,
    #[serde(rename = "gamemode_mp_survival")]
    Survival,
    #[serde(rename = "gamemode_mp_coopSurvival")]
    CoopSurvival,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyOptions {
    pub back: String,
    pub challenge: String,
    pub custom_seed: String,
    pub death_on_round_loss: bool,
    pub different_decks: bool,
    pub different_seeds: bool,
    pub disable_live_and_timer_hud: bool,
    pub gamemode: GameMode,
    pub gold_on_life_loss: bool,
    pub multiplayer_jokers: bool,
    pub no_gold_on_round_loss: bool,
    pub normal_bosses: bool,
    pub pvp_start_round: i32,
    pub ruleset: String,
    pub showdown_starting_antes: i32,
    pub stake: i32,
    pub starting_lives: u8,
    pub timer_base_seconds: i32,
    pub timer_increment_seconds: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindChoice {
    pub small: Option<String>,
    pub big: Option<String>,
    pub boss: Option<String>,
}

pub struct GameModeData {
    pub default_options: LobbyOptions,
    pub max_players: u8,
}

impl GameMode {
    pub fn get_data(&self) -> &GameModeData {
        match self {
            GameMode::Attrition => &ATTRITION_DATA,
            GameMode::Showdown => &SHOWDOWN_DATA,
            GameMode::Survival => &SURVIVAL_DATA,
            GameMode::CoopSurvival => &COOP_SURVIVAL_DATA,
        }
    }

    pub fn get_default_options(&self) -> LobbyOptions {
        self.get_data().default_options.clone()
    }
}

// Attrition game mode
static ATTRITION_DATA: LazyLock<GameModeData> = LazyLock::new(|| GameModeData {
    max_players: 2,
    default_options: LobbyOptions {
        back: String::from("Red Deck"),
        challenge: String::from(""),
        custom_seed: String::from("random"),
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Attrition,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 2,
        ruleset: String::from("ruleset_mp_standard"),
        showdown_starting_antes: 3,
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
});

// Showdown game mode
static SHOWDOWN_DATA: LazyLock<GameModeData> = LazyLock::new(|| GameModeData {
    max_players: 2,
    default_options: LobbyOptions {
        back: String::from("Red Deck"),
        challenge: String::from(""),
        custom_seed: String::from("random"),
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Showdown,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 2,
        ruleset: String::from("ruleset_mp_standard"),
        showdown_starting_antes: 3,
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
});

// Survival game mode
static SURVIVAL_DATA: LazyLock<GameModeData> = LazyLock::new(|| GameModeData {
    max_players: 2,
    default_options: LobbyOptions {
        back: String::from("Red Deck"),
        challenge: String::from(""),
        custom_seed: String::from("random"),
        death_on_round_loss: false,
        different_decks: false,
        different_seeds: false,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::Survival,
        gold_on_life_loss: true,
        multiplayer_jokers: true,
        no_gold_on_round_loss: false,
        normal_bosses: false,
        pvp_start_round: 20,
        ruleset: String::from("ruleset_mp_standard"),
        showdown_starting_antes: 3,
        stake: 1,
        starting_lives: 4,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
});

// CoopSurvival game mode
static COOP_SURVIVAL_DATA: LazyLock<GameModeData> = LazyLock::new(|| GameModeData {
    max_players: 6,
    default_options: LobbyOptions {
        back: String::from("Red Deck"),
        challenge: String::from(""),
        custom_seed: String::from("random"),
        death_on_round_loss: true,
        different_decks: true,
        different_seeds: true,
        disable_live_and_timer_hud: false,
        gamemode: GameMode::CoopSurvival,
        ruleset: String::from("ruleset_mp_coop"),
        gold_on_life_loss: false,
        multiplayer_jokers: false,
        no_gold_on_round_loss: true,
        normal_bosses: true,
        pvp_start_round: 2,
        showdown_starting_antes: 3,
        stake: 1,
        starting_lives: 2,
        timer_base_seconds: 150,
        timer_increment_seconds: 60,
    },
});

impl std::str::FromStr for GameMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Attrition" => Ok(GameMode::Attrition),
            "Showdown" => Ok(GameMode::Showdown),
            "Survival" => Ok(GameMode::Survival),
            "CoopSurvival" => Ok(GameMode::CoopSurvival),
            _ => Err(format!("Unknown game mode: {}", s)),
        }
    }
}

impl std::fmt::Display for GameMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameMode::Attrition => write!(f, "Attrition"),
            GameMode::Showdown => write!(f, "Showdown"),
            GameMode::Survival => write!(f, "Survival"),
            GameMode::CoopSurvival => write!(f, "CoopSurvival"),
        }
    }
}
